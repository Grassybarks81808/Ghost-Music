import { db } from "@/db";
import { downloadJobs, tracks } from "@/db/schema";
import { and, eq, inArray, desc } from "drizzle-orm";
import { mkdir, stat } from "node:fs/promises";
import { spawn, type ChildProcess } from "node:child_process";
import { randomUUID } from "node:crypto";
import path from "node:path";
import { ytdlpPath, ffmpegPath, validateYouTubeUrl } from "@/lib/youtube";
import { downloadsDirectory } from "@/lib/paths";
import type { Track } from "@/lib/catalog";

const running = new Map<string, ChildProcess>();

async function extractAudio(id: string, track: Track) {
  try {
    const directory = downloadsDirectory();
    await mkdir(directory, { recursive: true });
    const outputPath = path.join(directory, `${id}.mp3`);
    const sourceUrl = validateYouTubeUrl(track.sourceUrl!);
    const child = spawn(ytdlpPath, ["--no-playlist", "--no-warnings", "--socket-timeout", "15", "--retries", "1", "--max-filesize", "100M", "--newline", "--progress", "--progress-template", "download:ghost:%(progress.downloaded_bytes)s:%(progress.total_bytes,progress.total_bytes_estimate)s:%(progress._percent_str)s", "-f", "bestaudio/best", "-x", "--audio-format", "mp3", "--audio-quality", "192K", "--embed-metadata", "--embed-thumbnail", "--convert-thumbnails", "jpg", "--ffmpeg-location", ffmpegPath, "-o", path.join(directory, `${id}.%(ext)s`), "--", sourceUrl], { stdio: ["ignore", "pipe", "pipe"] });
    running.set(id, child);
    let lastUpdate = 0;
    let pending = Promise.resolve();
    let output = "";
    const timeout = setTimeout(() => child.kill("SIGKILL"), 240000);
    const update = (data: string) => {
      output = (output + data).slice(-8000);
      if (Date.now() - lastUpdate < 500) return;
      const matches = [...data.matchAll(/ghost:(\d+):(\d+|NA):\s*([\d.]+)%/g)];
      const match = matches.at(-1);
      if (match) {
        lastUpdate = Date.now();
        const progress = Math.min(100, Math.round(Number(match[3])));
        pending = pending.then(async () => { await db.update(downloadJobs).set({ status: "downloading", progress, downloadedBytes: Number(match[1]), totalBytes: Number(match[2]) || 0 }).where(eq(downloadJobs.id, id)); }).catch(() => {});
      } else if (data.includes("[ExtractAudio]") || data.includes("[EmbedThumbnail]")) {
        pending = pending.then(async () => { await db.update(downloadJobs).set({ status: "processing", progress: 100 }).where(eq(downloadJobs.id, id)); }).catch(() => {});
      }
    };
    child.stdout?.on("data", chunk => update(chunk.toString()));
    child.stderr?.on("data", chunk => update(chunk.toString()));
    child.on("error", async () => {
      clearTimeout(timeout); running.delete(id);
      await pending;
      await db.update(downloadJobs).set({ status: "error", error: "The extraction service could not start. Please try again." }).where(eq(downloadJobs.id, id));
    });
    child.on("close", async code => {
      clearTimeout(timeout); running.delete(id);
      await pending;
      try {
        if (code !== 0) throw new Error(output.includes("Sign in") || output.includes("403") ? "YouTube restricted this download. Try another public track. Ghost Sessions originals can always be saved." : "This track could not be extracted. It may be restricted, unavailable, or exceed the download limit.");
        const file = await stat(outputPath);
        await db.update(downloadJobs).set({ status: "ready", progress: 100, outputPath, downloadedBytes: file.size, totalBytes: file.size, metadata: { title: track.title, artist: track.artist, album: track.album, format: "MP3", bitrate: "192 kbps", size: file.size, source: "YouTube", artwork: "Embedded when available" } }).where(eq(downloadJobs.id, id));
      } catch (error) {
        await db.update(downloadJobs).set({ status: "error", error: error instanceof Error ? error.message : "Download failed. Please try again." }).where(eq(downloadJobs.id, id));
      }
    });
  } catch {
    await db.update(downloadJobs).set({ status: "error", error: "The audio extraction service could not prepare this download." }).where(eq(downloadJobs.id, id));
  }
}

export async function startDownload(listenerId: string, trackId: string, permitted = false) {
  const [track] = await db.select().from(tracks).where(eq(tracks.id, trackId));
  if (!track) throw new Error("Track not found.");
  if (track.source === "youtube" && !permitted) throw new Error("Please confirm you have permission to download this audio.");
  if (track.duration > 1800) throw new Error("Downloads are limited to tracks under 30 minutes.");
  const [existing] = await db.select().from(downloadJobs).where(and(eq(downloadJobs.listenerId, listenerId), eq(downloadJobs.trackId, trackId), inArray(downloadJobs.status, ["ready", "downloading", "processing", "queued"]))).orderBy(desc(downloadJobs.createdAt));
  if (existing) {
    if (existing.status === "ready" && existing.outputPath) {
      try { await stat(existing.outputPath); return existing; } catch { await db.update(downloadJobs).set({ status: "error", error: "Cached file expired. Download again." }).where(eq(downloadJobs.id, existing.id)); }
    } else if (Date.now() - existing.createdAt.getTime() < 240000) return existing;
    else await db.update(downloadJobs).set({ status: "error", error: "This extraction timed out. Please retry." }).where(eq(downloadJobs.id, existing.id));
  }
  const active = await db.select().from(downloadJobs).where(and(eq(downloadJobs.listenerId, listenerId), inArray(downloadJobs.status, ["queued", "downloading", "processing"])));
  if (active.length >= 2 || running.size >= 8) throw new Error("The download queue is busy. Let a current download finish first.");
  const id = randomUUID();
  const [job] = await db.insert(downloadJobs).values({ id, listenerId, trackId, status: "queued" }).returning();
  if (track.source === "ghost" && track.audioPath) {
    const outputPath = path.join(process.cwd(), "public", track.audioPath);
    const info = await stat(outputPath);
    const [ready] = await db.update(downloadJobs).set({ status: "ready", progress: 100, downloadedBytes: info.size, totalBytes: info.size, outputPath, metadata: { title: track.title, artist: track.artist, album: track.album, format: "MP3", bitrate: "160 kbps", size: info.size, source: "Ghost Sessions original", artwork: "Embedded", duration: track.duration } }).where(eq(downloadJobs.id, id)).returning();
    return ready;
  }
  void extractAudio(id, track);
  return job;
}
