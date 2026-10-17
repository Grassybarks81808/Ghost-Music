import { db } from "@/db";
import { tracks } from "@/db/schema";
import { apiError, ensureCatalog } from "@/lib/server";
import { fetchYouTubeMetadata } from "@/lib/youtube";

export const dynamic = "force-dynamic";
export async function GET(request: Request) {
  try {
    const url = new URL(request.url);
    const query = url.searchParams.get("q")?.trim().slice(0, 300);
    if (!query) return Response.json({ tracks: [] });
    await ensureCatalog();
    if (!/^https?:\/\//i.test(query) && url.searchParams.get("source") !== "youtube") {
      const all = await db.select().from(tracks);
      const local = all.filter(track => `${track.title} ${track.artist} ${track.album} ${track.genre}`.toLowerCase().includes(query.toLowerCase()));
      if (local.length) return Response.json({ tracks: local, source: "library" });
    }
    const result = await fetchYouTubeMetadata(query);
    await db.insert(tracks).values(result.tracks).onConflictDoNothing();
    return Response.json({ tracks: result.tracks, source: "youtube" });
  } catch (error) { return apiError(error, 502); }
}
