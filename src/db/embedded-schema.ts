/**
 * Schema for the embedded PGlite database used by the packaged desktop and
 * Android builds, which have no PostgreSQL server to connect to.
 *
 * Keep this in sync with src/db/schema.ts. PostgreSQL deployments still use
 * `drizzle-kit push`; this DDL only bootstraps a fresh local library file.
 */
export const EMBEDDED_SCHEMA = /* sql */ `
  CREATE TABLE IF NOT EXISTS ghost_listeners (
    id text PRIMARY KEY,
    created_at timestamptz NOT NULL DEFAULT now()
  );

  CREATE TABLE IF NOT EXISTS ghost_tracks (
    id text PRIMARY KEY,
    title text NOT NULL,
    artist text NOT NULL,
    album text NOT NULL DEFAULT 'Ghost Music',
    duration integer NOT NULL DEFAULT 0,
    artwork text NOT NULL,
    source text NOT NULL DEFAULT 'ghost',
    source_url text,
    audio_path text,
    genre text NOT NULL DEFAULT 'Electronic',
    created_at timestamptz NOT NULL DEFAULT now()
  );

  CREATE TABLE IF NOT EXISTS ghost_collections (
    listener_id text NOT NULL,
    track_id text NOT NULL REFERENCES ghost_tracks(id) ON DELETE CASCADE,
    kind text NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (listener_id, track_id, kind)
  );

  CREATE TABLE IF NOT EXISTS ghost_playlists (
    id text PRIMARY KEY,
    listener_id text NOT NULL,
    name text NOT NULL,
    description text NOT NULL DEFAULT 'A little collection of your favorite hauntings.',
    artwork text NOT NULL DEFAULT '/images/after-hours.png',
    created_at timestamptz NOT NULL DEFAULT now()
  );

  CREATE TABLE IF NOT EXISTS ghost_playlist_tracks (
    playlist_id text NOT NULL REFERENCES ghost_playlists(id) ON DELETE CASCADE,
    track_id text NOT NULL REFERENCES ghost_tracks(id) ON DELETE CASCADE,
    added_at timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (playlist_id, track_id)
  );

  CREATE TABLE IF NOT EXISTS ghost_downloads (
    id text PRIMARY KEY,
    listener_id text NOT NULL,
    track_id text NOT NULL REFERENCES ghost_tracks(id),
    status text NOT NULL DEFAULT 'queued',
    progress integer NOT NULL DEFAULT 0,
    downloaded_bytes integer NOT NULL DEFAULT 0,
    total_bytes integer NOT NULL DEFAULT 0,
    output_path text,
    error text,
    metadata jsonb,
    created_at timestamptz NOT NULL DEFAULT now()
  );

  CREATE INDEX IF NOT EXISTS ghost_collections_listener_idx ON ghost_collections(listener_id, kind);
  CREATE INDEX IF NOT EXISTS ghost_playlists_listener_idx ON ghost_playlists(listener_id);
  CREATE INDEX IF NOT EXISTS ghost_downloads_listener_idx ON ghost_downloads(listener_id, created_at);
`;
