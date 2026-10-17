import { execFile } from "node:child_process";
import { promisify } from "node:util";
import path from "node:path";
import type { Track } from "@/lib/catalog";

const run = promisify(execFile);
const windows = process.platform === "win32";
const binary = (name: string) => path.join(process.cwd(), `node_modules/${name}${windows ? ".exe" : ""}`);
// The bundled tools live next to the server in packaged builds; the env vars let
// a host override them (for example a system-wide FFmpeg on Linux).
export const ytdlpPath = process.env.GHOST_YTDLP_PATH || binary("youtube-dl-exec/bin/yt-dlp");
export const ffmpegPath = process.env.GHOST_FFMPEG_PATH || binary("ffmpeg-static/ffmpeg");

type VideoInfo = { id?: string; title?: string; uploader?: string; artist?: string; channel?: string; album?: string; duration?: number; thumbnail?: string; thumbnails?: { url: string }[]; url?: string; webpage_url?: string; entries?: VideoInfo[]; };

export function validateYouTubeUrl(value: string) {
  let url: URL;
  try { url = new URL(value); } catch { throw new Error("Paste a valid YouTube video or playlist link."); }
  if (!["https:", "http:"].includes(url.protocol) || !["youtube.com", "www.youtube.com", "music.youtube.com", "m.youtube.com", "youtu.be"].includes(url.hostname) || url.username || url.password || url.port) throw new Error("Only YouTube video and playlist links are supported.");
  url.protocol = "https:";
  return url.toString();
}

export async function fetchYouTubeMetadata(query: string): Promise<{ name: string; tracks: Track[] }> {
  const isUrl = /^https?:\/\//i.test(query);
  const target = isUrl ? validateYouTubeUrl(query) : `ytsearch12:${query.slice(0, 150)}`;
  let info: VideoInfo;
  try {
    const { stdout } = await run(ytdlpPath, ["--flat-playlist", "--dump-single-json", "--skip-download", "--no-warnings", "--socket-timeout", "12", "--retries", "1", "--playlist-end", "30", "--", target], { timeout: 45000, maxBuffer: 8 * 1024 * 1024 });
    info = JSON.parse(stdout);
  } catch {
    if (isUrl) {
      const u = new URL(target);
      const id = u.hostname === "youtu.be" ? u.pathname.slice(1) : u.searchParams.get("v") || u.pathname.match(/\/(?:shorts|embed)\/([^/]+)/)?.[1];
      if (id && /^[A-Za-z0-9_-]{11}$/.test(id)) {
        try {
          const response = await fetch(`https://www.youtube.com/oembed?url=${encodeURIComponent(`https://www.youtube.com/watch?v=${id}`)}&format=json`, { signal: AbortSignal.timeout(10000) });
          if (!response.ok) throw new Error("unavailable");
          const data = await response.json();
          info = { id, title: data.title, uploader: data.author_name, thumbnail: data.thumbnail_url, duration: 0 };
        } catch { throw new Error("YouTube could not provide this track. It may be private, restricted, or temporarily unavailable. Your Ghost Sessions library is still ready to play."); }
      } else throw new Error("YouTube is limiting playlist imports right now. Try a public video link or return in a moment.");
    } else throw new Error("YouTube search is temporarily unavailable. Try a direct YouTube link, or search the Ghost Sessions collection.");
  }
  const entries = info.entries || [info];
  const result = entries.filter(item => item?.id && /^[A-Za-z0-9_-]{11}$/.test(item.id)).map(item => ({
    id: `yt-${item.id}`, title: item.title || "Untitled track", artist: item.artist || item.uploader || item.channel || "YouTube artist", album: item.album || "YouTube import", duration: Math.round(item.duration || 0),
    artwork: item.thumbnail || item.thumbnails?.at(-1)?.url || `https://i.ytimg.com/vi/${item.id}/hqdefault.jpg`,
    source: "youtube", sourceUrl: `https://www.youtube.com/watch?v=${item.id}`, audioPath: null, genre: "Imported",
  }));
  if (!result.length) throw new Error("No playable public tracks were found in this link.");
  return { name: info.title || "YouTube discoveries", tracks: result };
}

const streamCache = new Map<string, { url: string; expires: number }>();
export async function resolveStream(sourceUrl: string) {
  const cached = streamCache.get(sourceUrl);
  if (cached && cached.expires > Date.now()) return cached.url;
  try {
    const { stdout } = await run(ytdlpPath, ["--no-playlist", "--no-warnings", "--socket-timeout", "12", "--retries", "1", "-f", "bestaudio[ext=m4a]/bestaudio/best", "--get-url", "--", validateYouTubeUrl(sourceUrl)], { timeout: 35000, maxBuffer: 1024 * 1024 });
    const url = stdout.trim().split("\n")[0];
    const host = new URL(url).hostname;
    if (!host.endsWith(".googlevideo.com") && host !== "googlevideo.com") throw new Error("Unsupported stream host");
    streamCache.set(sourceUrl, { url, expires: Date.now() + 15 * 60 * 1000 });
    return url;
  } catch { throw new Error("This YouTube stream is restricted or unavailable. Try another track."); }
}
