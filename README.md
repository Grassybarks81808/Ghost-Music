# 👻 Ghost Music

Ghost Music is a Windows-first music and video discovery/player project written in Rust, with **yt-dlp** handling search metadata and media downloads.

## Current prototype

- Dark, red-accented Ghost Music interface.
- A 15-second startup animation: the ghost rotates, headphones drop into place, then audio-wave tones flow around it.
- Search YouTube using `yt-dlp` and display titles, channel names, durations, and thumbnail URLs.
- Download audio as MP3 or video as MP4 (where supported by the source and installed media tools).
- A ghost-shaped playback progress marker in the player timeline.
- Windows release builds through GitHub Actions.

> This is an early prototype. It does not include every feature of YouTube, and playback/metadata availability depends on yt-dlp and the source site.

## Build on Windows

1. Install the stable Rust toolchain from [rustup.rs](https://rustup.rs/).
2. Install [yt-dlp](https://github.com/yt-dlp/yt-dlp/releases/latest) and put `yt-dlp.exe` on your PATH, or place it beside the app executable.
3. For MP3 conversion, install [FFmpeg](https://ffmpeg.org/download.html) and put `ffmpeg.exe` on your PATH, or place it beside the app executable.
4. In this repository, run:

   ```powershell
   cargo run --release
   ```

## Search and downloads

Search uses yt-dlp's YouTube search extractor. Downloads are saved to the user's Downloads folder under `Ghost Music`.

- Audio: yt-dlp extracts audio and asks FFmpeg to convert it to MP3.
- Video: yt-dlp downloads the best available video/audio combination and merges to MP4 when possible.

Only download content you have permission to save, and follow the source site's terms and applicable law.

## Create a Windows release

Push a version tag such as `v0.1.0`. The workflow builds the Windows executable and attaches it to a GitHub Release. The executable expects `yt-dlp.exe` (and FFmpeg for MP3 conversion) alongside it or available on PATH.

## Stack

- Rust
- eframe / egui for the native desktop UI
- yt-dlp as a child process for media search and download
- GitHub Actions for Windows builds

## License

MIT — see [LICENSE](LICENSE).
