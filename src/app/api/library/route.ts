import { db } from "@/db";
import { playlists, playlistTracks } from "@/db/schema";
import { and, eq } from "drizzle-orm";
import { apiError, listenerFrom, getLibrary, recordCollection, createPlaylist, ensureCatalog } from "@/lib/server";

export const dynamic = "force-dynamic";
export async function GET(request: Request) {
  try { return Response.json(await getLibrary(listenerFrom(request))); }
  catch (error) { return apiError(error, 500); }
}

export async function POST(request: Request) {
  try {
    const listenerId = listenerFrom(request);
    const body = await request.json();
    await ensureCatalog();
    if (body.action === "like" || body.action === "history") {
      if (typeof body.trackId !== "string") throw new Error("Choose a track first.");
      await recordCollection(listenerId, body.trackId, body.action, body.active !== false);
      return Response.json({ ok: true });
    }
    if (body.action === "create-playlist") {
      if (typeof body.name !== "string" || !body.name.trim()) throw new Error("Give your playlist a name.");
      const id = await createPlaylist(listenerId, body.name, typeof body.description === "string" ? body.description : "", Array.isArray(body.trackIds) ? body.trackIds.filter((id: unknown) => typeof id === "string") : []);
      return Response.json({ id });
    }
    if (["add-to-playlist", "remove-from-playlist", "delete-playlist"].includes(body.action)) {
      const [playlist] = await db.select().from(playlists).where(and(eq(playlists.id, body.playlistId), eq(playlists.listenerId, listenerId)));
      if (!playlist) throw new Error("Playlist not found.");
      if (body.action === "delete-playlist") await db.delete(playlists).where(eq(playlists.id, playlist.id));
      else if (body.action === "add-to-playlist") await db.insert(playlistTracks).values({ playlistId: playlist.id, trackId: body.trackId }).onConflictDoNothing();
      else await db.delete(playlistTracks).where(and(eq(playlistTracks.playlistId, playlist.id), eq(playlistTracks.trackId, body.trackId)));
      return Response.json({ ok: true });
    }
    throw new Error("Unknown library action.");
  } catch (error) { return apiError(error); }
}
