import { db } from "@/db";
import { tracks } from "@/db/schema";
import { apiError, listenerFrom, createPlaylist } from "@/lib/server";
import { fetchYouTubeMetadata, validateYouTubeUrl } from "@/lib/youtube";

export async function POST(request: Request) {
  try {
    const listenerId = listenerFrom(request);
    const body = await request.json();
    if (typeof body.url !== "string" || body.url.length > 2000) throw new Error("Enter a YouTube link.");
    const result = await fetchYouTubeMetadata(validateYouTubeUrl(body.url));
    await db.insert(tracks).values(result.tracks).onConflictDoNothing();
    const playlistId = await createPlaylist(listenerId, result.name, "Imported from YouTube. Your favorites, in a new home.", result.tracks.map(track => track.id));
    return Response.json({ playlistId, count: result.tracks.length, tracks: result.tracks });
  } catch (error) { return apiError(error, 400); }
}
