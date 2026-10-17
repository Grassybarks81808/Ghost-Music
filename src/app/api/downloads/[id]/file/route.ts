import { db } from "@/db";
import { downloadJobs, tracks } from "@/db/schema";
import { and, eq } from "drizzle-orm";
import { apiError, listenerFrom, recordCollection } from "@/lib/server";
import { serveAudioFile } from "@/lib/audio-file";

export const dynamic = "force-dynamic";
export async function GET(request: Request, context: { params: Promise<{ id: string }> }) {
  try {
    const listenerId = listenerFrom(request);
    const { id } = await context.params;
    const [result] = await db.select({ job: downloadJobs, track: tracks }).from(downloadJobs).innerJoin(tracks, eq(downloadJobs.trackId, tracks.id)).where(and(eq(downloadJobs.id, id), eq(downloadJobs.listenerId, listenerId)));
    if (!result || result.job.status !== "ready" || !result.job.outputPath) return Response.json({ error: "This download is not ready yet." }, { status: 404 });
    await recordCollection(listenerId, result.track.id, "download", true);
    const filename = `${result.track.artist} - ${result.track.title}`.replace(/[\\/:*?"<>|]/g, "").slice(0, 160) + ".mp3";
    return serveAudioFile(result.job.outputPath, request, filename);
  } catch (error) { return apiError(error, 500); }
}
