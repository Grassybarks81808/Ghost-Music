import { db } from "@/db";
import { playlists, playlistTracks, tracks } from "@/db/schema";
import { eq } from "drizzle-orm";
import { apiError } from "@/lib/server";

export async function GET(_request: Request, context: { params: Promise<{ id: string }> }) {
  try {
    const { id } = await context.params;
    const [playlist] = await db.select({ id: playlists.id, name: playlists.name, description: playlists.description, artwork: playlists.artwork }).from(playlists).where(eq(playlists.id, id));
    if (!playlist) return Response.json({ error: "This playlist is no longer available." }, { status: 404 });
    const items = await db.select({ track: tracks }).from(playlistTracks).innerJoin(tracks, eq(playlistTracks.trackId, tracks.id)).where(eq(playlistTracks.playlistId, id)).orderBy(playlistTracks.addedAt);
    return Response.json({ playlist: { ...playlist, trackIds: items.map(item => item.track.id) }, tracks: items.map(item => item.track) });
  } catch (error) { return apiError(error, 500); }
}
