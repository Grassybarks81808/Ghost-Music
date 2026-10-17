import { db } from "@/db";
import { downloadJobs } from "@/db/schema";
import { desc, eq } from "drizzle-orm";
import { apiError, listenerFrom } from "@/lib/server";
import { startDownload } from "@/lib/downloads";

export const dynamic = "force-dynamic";
const publicJob = (job: typeof downloadJobs.$inferSelect) => ({ id: job.id, trackId: job.trackId, status: job.status, progress: job.progress, downloadedBytes: job.downloadedBytes, totalBytes: job.totalBytes, error: job.error, metadata: job.metadata });
export async function GET(request: Request) {
  try {
    const jobs = await db.select().from(downloadJobs).where(eq(downloadJobs.listenerId, listenerFrom(request))).orderBy(desc(downloadJobs.createdAt)).limit(40);
    return Response.json({ jobs: jobs.map(publicJob) });
  } catch (error) { return apiError(error, 500); }
}
export async function POST(request: Request) {
  try {
    const listenerId = listenerFrom(request);
    const body = await request.json();
    if (typeof body.trackId !== "string") throw new Error("Select a track to download.");
    return Response.json({ job: publicJob(await startDownload(listenerId, body.trackId, body.permitted === true)) });
  } catch (error) { return apiError(error); }
}
