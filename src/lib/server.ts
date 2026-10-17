import { db } from "@/db";
import { tracks, collections, playlists, playlistTracks, listeners } from "@/db/schema";
import { catalog, initialPlaylists } from "@/lib/catalog";
import { and, eq, desc } from "drizzle-orm";
import { randomUUID } from "node:crypto";

let catalogReady: Promise<void> | undefined;
export function ensureCatalog() {
  if (!catalogReady) catalogReady = db.insert(tracks).values(catalog).onConflictDoNothing().then(() => {}).catch(error => { catalogReady = undefined; throw error; });
  return catalogReady;
}

export function listenerFrom(request: Request) {
  const value = request.headers.get("x-ghost-listener") || "";
  if (!/^[a-f0-9-]{36}$/i.test(value)) throw new Error("A valid listener session is required.");
  return value;
}

export async function getLibrary(listenerId: string) {
  await ensureCatalog();
  await db.transaction(async tx => {
    const [created] = await tx.insert(listeners).values({ id: listenerId }).onConflictDoNothing().returning();
    if (!created) return;
    for (let i = 0; i < initialPlaylists.length; i++) {
      const starter = initialPlaylists[i];
      const id = `starter-${listenerId}-${i}`;
      await tx.insert(playlists).values({ id, listenerId, name: starter.name, artwork: starter.artwork }).onConflictDoNothing();
      await tx.insert(playlistTracks).values(starter.trackIds.map(trackId => ({ playlistId: id, trackId }))).onConflictDoNothing();
    }
  });
  const userPlaylists = await db.select().from(playlists).where(eq(playlists.listenerId, listenerId));
  const entries = await db.select().from(collections).where(eq(collections.listenerId, listenerId)).orderBy(desc(collections.createdAt));
  const allTracks = await db.select().from(tracks).orderBy(tracks.createdAt);
  const lists = await Promise.all(userPlaylists.map(async playlist => ({ ...playlist, trackIds: (await db.select().from(playlistTracks).where(eq(playlistTracks.playlistId, playlist.id)).orderBy(playlistTracks.addedAt)).map(row => row.trackId) })));
  return { tracks: allTracks, playlists: lists, liked: entries.filter(e => e.kind === "like").map(e => e.trackId), history: entries.filter(e => e.kind === "history").map(e => e.trackId), downloaded: entries.filter(e => e.kind === "download").map(e => e.trackId) };
}

export async function recordCollection(listenerId: string, trackId: string, kind: string, active: boolean) {
  if (!["like", "history", "download"].includes(kind)) throw new Error("Unknown collection.");
  const condition = and(eq(collections.listenerId, listenerId), eq(collections.trackId, trackId), eq(collections.kind, kind));
  if (active) {
    await db.insert(collections).values({ listenerId, trackId, kind }).onConflictDoUpdate({ target: [collections.listenerId, collections.trackId, collections.kind], set: { createdAt: new Date() } });
  } else await db.delete(collections).where(condition);
}

export async function createPlaylist(listenerId: string, name: string, description = "", trackIds: string[] = []) {
  const id = randomUUID();
  await db.transaction(async tx => {
    await tx.insert(playlists).values({ id, listenerId, name: name.trim().slice(0, 80), description: description.trim().slice(0, 400) });
    if (trackIds.length) await tx.insert(playlistTracks).values(trackIds.slice(0, 100).map(trackId => ({ playlistId: id, trackId }))).onConflictDoNothing();
  });
  return id;
}

export function apiError(error: unknown, status = 400) {
  console.error("Ghost Music:", error instanceof Error ? error.message : error);
  return Response.json({ error: error instanceof Error ? error.message : "Something went wrong. Please try again." }, { status });
}
