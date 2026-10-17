import { pgTable, text, integer, timestamp, primaryKey, jsonb } from "drizzle-orm/pg-core";

export const listeners = pgTable("ghost_listeners", {
  id: text("id").primaryKey(),
  createdAt: timestamp("created_at").defaultNow().notNull(),
});

export const tracks = pgTable("ghost_tracks", {
  id: text("id").primaryKey(),
  title: text("title").notNull(),
  artist: text("artist").notNull(),
  album: text("album").notNull().default("Ghost Music"),
  duration: integer("duration").notNull().default(0),
  artwork: text("artwork").notNull(),
  source: text("source").notNull().default("ghost"),
  sourceUrl: text("source_url"),
  audioPath: text("audio_path"),
  genre: text("genre").notNull().default("Electronic"),
  createdAt: timestamp("created_at").defaultNow().notNull(),
});

export const collections = pgTable("ghost_collections", {
  listenerId: text("listener_id").notNull(),
  trackId: text("track_id").notNull().references(() => tracks.id, { onDelete: "cascade" }),
  kind: text("kind").notNull(),
  createdAt: timestamp("created_at").defaultNow().notNull(),
}, (table) => [primaryKey({ columns: [table.listenerId, table.trackId, table.kind] })]);

export const playlists = pgTable("ghost_playlists", {
  id: text("id").primaryKey(),
  listenerId: text("listener_id").notNull(),
  name: text("name").notNull(),
  description: text("description").notNull().default("A little collection of your favorite hauntings."),
  artwork: text("artwork").notNull().default("/images/after-hours.png"),
  createdAt: timestamp("created_at").defaultNow().notNull(),
});

export const playlistTracks = pgTable("ghost_playlist_tracks", {
  playlistId: text("playlist_id").notNull().references(() => playlists.id, { onDelete: "cascade" }),
  trackId: text("track_id").notNull().references(() => tracks.id, { onDelete: "cascade" }),
  addedAt: timestamp("added_at").defaultNow().notNull(),
}, (table) => [primaryKey({ columns: [table.playlistId, table.trackId] })]);

export const downloadJobs = pgTable("ghost_downloads", {
  id: text("id").primaryKey(),
  listenerId: text("listener_id").notNull(),
  trackId: text("track_id").notNull().references(() => tracks.id),
  status: text("status").notNull().default("queued"),
  progress: integer("progress").notNull().default(0),
  downloadedBytes: integer("downloaded_bytes").notNull().default(0),
  totalBytes: integer("total_bytes").notNull().default(0),
  outputPath: text("output_path"),
  error: text("error"),
  metadata: jsonb("metadata").$type<Record<string, string | number>>(),
  createdAt: timestamp("created_at").defaultNow().notNull(),
});
