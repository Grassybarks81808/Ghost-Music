import { db } from "@/db";
import { tracks } from "@/db/schema";
import { eq } from "drizzle-orm";
import path from "node:path";
import { serveAudioFile } from "@/lib/audio-file";
import { resolveStream } from "@/lib/youtube";
import { apiError } from "@/lib/server";

export const dynamic = "force-dynamic";
export async function GET(request: Request, context: { params: Promise<{ id: string }> }) {
  try {
    const { id } = await context.params;
    const [track] = await db.select().from(tracks).where(eq(tracks.id, id));
    if (!track) return Response.json({ error: "Track not found." }, { status: 404 });
    if (track.source === "ghost" && track.audioPath) return serveAudioFile(path.join(process.cwd(), "public", track.audioPath), request);
    if (!track.sourceUrl) throw new Error("No audio source is available.");
    const streamUrl = await resolveStream(track.sourceUrl);
    const upstream = await fetch(streamUrl, { headers: request.headers.has("range") ? { Range: request.headers.get("range")! } : {}, signal: request.signal });
    if (!upstream.ok) throw new Error("The source could not stream this track. Try again in a moment.");
    const headers = new Headers({ "Cache-Control": "no-store", "Accept-Ranges": "bytes" });
    for (const key of ["content-type", "content-length", "content-range"]) { const value = upstream.headers.get(key); if (value) headers.set(key, value); }
    return new Response(upstream.body, { status: upstream.status, headers });
  } catch (error) { return apiError(error, 502); }
}
