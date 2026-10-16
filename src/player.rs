//! Audio playback.
//!
//! Audio is fetched from the local API server (`/stream/{id}`), which supports
//! byte ranges, so [`HttpSource`] can seek and symphonia can decode mp4/m4a,
//! mp3, opus/webm, vorbis and flac without any codec installed on the machine.

use std::io::{self, Read, Seek, SeekFrom};
use std::sync::mpsc::Sender;
use std::sync::OnceLock;
use std::thread;
use std::time::Duration;

use rodio::{Decoder, OutputStream, OutputStreamHandle, Sink};

use crate::model::Track;

/// Decoder over a remote, range-capable HTTP resource.
pub type RemoteDecoder = Decoder<HttpSource>;

/// Result of opening a stream on a worker thread.
pub enum PlayerUpdate {
    Opened { key: String, decoder: Box<RemoteDecoder> },
    Failed { key: String, error: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlaybackState {
    /// No output device (or none available yet).
    Unavailable,
    Idle,
    Loading,
    Playing,
    Paused,
    Finished,
}

pub struct Player {
    stream: Option<OutputStream>,
    handle: Option<OutputStreamHandle>,
    sink: Option<Sink>,
    volume: f32,
    pub state: PlaybackState,
    /// Track currently loaded into the sink.
    pub track: Option<Track>,
    /// Key of the track being opened (set while `state == Loading`).
    pub loading: Option<String>,
    pub error: Option<String>,
}

impl Player {
    pub fn new(volume: f32) -> Self {
        match OutputStream::try_default() {
            Ok((stream, handle)) => Self {
                stream: Some(stream),
                handle: Some(handle),
                sink: None,
                volume,
                state: PlaybackState::Idle,
                track: None,
                loading: None,
                error: None,
            },
            Err(error) => Self {
                stream: None,
                handle: None,
                sink: None,
                volume,
                state: PlaybackState::Unavailable,
                track: None,
                loading: None,
                error: Some(format!("No audio output device: {error}")),
            },
        }
    }

    pub fn is_available(&self) -> bool {
        self.handle.is_some()
    }

    /// Attach a freshly opened decoder and start playing.
    pub fn start(&mut self, key: &str, track: Track, decoder: RemoteDecoder) -> Result<(), String> {
        self.loading = None;
        let Some(handle) = self.handle.as_ref() else {
            return Err("No audio output device was found.".to_string());
        };
        self.stop();
        let sink = Sink::try_new(handle).map_err(|error| format!("Could not start audio: {error}"))?;
        sink.set_volume(self.volume);
        sink.append(decoder);
        sink.play();
        self.sink = Some(sink);
        self.state = PlaybackState::Playing;
        self.track = Some(track);
        self.error = None;
        let _ = key;
        Ok(())
    }

    pub fn pause(&mut self) {
        if let Some(sink) = &self.sink {
            sink.pause();
            self.state = PlaybackState::Paused;
        }
    }

    pub fn resume(&mut self) {
        if let Some(sink) = &self.sink {
            sink.play();
            self.state = PlaybackState::Playing;
        }
    }

    pub fn toggle(&mut self) {
        match self.state {
            PlaybackState::Playing => self.pause(),
            PlaybackState::Paused => self.resume(),
            _ => {}
        }
    }

    pub fn stop(&mut self) {
        if let Some(sink) = self.sink.take() {
            sink.stop();
        }
        if self.state != PlaybackState::Unavailable {
            self.state = PlaybackState::Idle;
        }
    }

    pub fn is_playing(&self) -> bool {
        self.state == PlaybackState::Playing
    }

    /// True when the current source has run out of samples.
    pub fn finished(&self) -> bool {
        matches!(self.state, PlaybackState::Playing | PlaybackState::Paused)
            && self.sink.as_ref().map(Sink::empty).unwrap_or(true)
    }

    pub fn position(&self) -> f64 {
        self.sink
            .as_ref()
            .map(|sink| sink.get_pos().as_secs_f64())
            .unwrap_or(0.0)
    }

    /// Seek inside the current track. Fails when the source is not seekable.
    pub fn seek(&mut self, seconds: f64) -> Result<(), String> {
        let Some(sink) = self.sink.as_ref() else {
            return Err("Nothing is playing.".to_string());
        };
        let position = seconds.max(0.0);
        sink.try_seek(Duration::from_secs_f64(position))
            .map_err(|error| format!("Cannot seek: {error}"))
    }

    pub fn set_volume(&mut self, volume: f32) {
        self.volume = volume.clamp(0.0, 1.0);
        if let Some(sink) = &self.sink {
            sink.set_volume(self.volume);
        }
    }

    pub fn volume(&self) -> f32 {
        self.volume
    }
}

/// Open a stream URL (the local `/stream/{id}` endpoint) on a worker thread and
/// hand the decoder to the UI thread.
pub fn spawn_open(tx: Sender<PlayerUpdate>, key: String, url: String) {
    thread::spawn(move || {
        let result = HttpSource::open(&url).and_then(|source| {
            Decoder::new(source).map_err(|error| format!("This audio format could not be decoded: {error}"))
        });
        let update = match result {
            Ok(decoder) => PlayerUpdate::Opened { key, decoder: Box::new(decoder) },
            Err(error) => PlayerUpdate::Failed { key, error },
        };
        let _ = tx.send(update);
    });
}

fn agent() -> &'static ureq::Agent {
    static AGENT: OnceLock<ureq::Agent> = OnceLock::new();
    AGENT.get_or_init(|| {
        ureq::AgentBuilder::new()
            .timeout_connect(Duration::from_secs(15))
            // Only stall-detection: long tracks read for minutes.
            .timeout_read(Duration::from_secs(30))
            .user_agent("GhostMusic/1.0")
            .build()
    })
}

/// A `Read + Seek` view over an HTTP resource that supports byte ranges.
/// Backwards seeks reconnect with a `Range:` header instead of re-downloading.
pub struct HttpSource {
    url: String,
    reader: Option<Box<dyn Read + Send + Sync>>,
    len: Option<u64>,
    pos: u64,
}

impl HttpSource {
    pub fn open(url: &str) -> Result<Self, String> {
        let response = agent()
            .get(url)
            .call()
            .map_err(|error| format!("Could not open the audio stream: {error}"))?;
        if response.status() >= 400 {
            return Err(format!(
                "The audio stream answered HTTP {} - the link may have expired, press play again.",
                response.status()
            ));
        }
        let len = response
            .header("Content-Length")
            .and_then(|value| value.parse::<u64>().ok());
        Ok(Self {
            url: url.to_string(),
            reader: Some(response.into_reader()),
            len,
            pos: 0,
        })
    }

    pub fn len(&self) -> Option<u64> {
        self.len
    }

    /// Drop the current body and re-request from `offset`.
    fn reopen(&mut self, offset: u64) -> io::Result<()> {
        self.reader = None;
        let response = agent()
            .get(&self.url)
            .set("Range", &format!("bytes={offset}-"))
            .call()
            .map_err(|error| io::Error::other(format!("stream reconnect failed: {error}")))?;
        let status = response.status();
        let mut reader = response.into_reader();
        if status == 200 && offset > 0 {
            // Server ignored the range: skip forward instead.
            let mut remaining = offset;
            let mut buffer = [0u8; 16 * 1024];
            while remaining > 0 {
                let want = (buffer.len() as u64).min(remaining) as usize;
                let read = reader.read(&mut buffer[..want])?;
                if read == 0 {
                    break;
                }
                remaining -= read as u64;
            }
        }
        self.reader = Some(reader);
        self.pos = offset;
        Ok(())
    }
}

impl Read for HttpSource {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let Some(reader) = self.reader.as_mut() else {
            return Ok(0);
        };
        match reader.read(buf) {
            Ok(0) => Ok(0),
            Ok(read) => {
                self.pos += read as u64;
                Ok(read)
            }
            Err(error) => Err(error),
        }
    }
}

impl Seek for HttpSource {
    fn seek(&mut self, position: SeekFrom) -> io::Result<u64> {
        let target = match position {
            SeekFrom::Start(offset) => offset as i64,
            SeekFrom::Current(delta) => self.pos as i64 + delta,
            SeekFrom::End(delta) => match self.len {
                Some(len) => len as i64 + delta,
                None => {
                    return Err(io::Error::new(
                        io::ErrorKind::Unsupported,
                        "stream length is unknown",
                    ))
                }
            },
        }
        .max(0) as u64;

        if Some(target) == self.len {
            return Ok(target);
        }
        if target != self.pos || self.reader.is_none() {
            self.reopen(target)?;
        }
        Ok(target)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seek_maths_without_network() {
        let mut source = HttpSource {
            url: "http://127.0.0.1:1/none".to_string(),
            reader: None,
            len: Some(1000),
            pos: 0,
        };
        // Reading with no reader is a clean EOF.
        let mut buffer = [0u8; 8];
        assert_eq!(source.read(&mut buffer).unwrap(), 0);
        // Seeking to the very end is allowed without a request.
        assert_eq!(source.seek(SeekFrom::Start(1000)).unwrap(), 1000);
        assert_eq!(source.seek(SeekFrom::End(-1000)).unwrap(), 0);
    }
}
