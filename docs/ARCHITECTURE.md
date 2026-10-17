# Architecture

Ghost Music is one web application with two thin native shells. Everything
interesting — the library, the search, the extraction pipeline, the player, the
offline cache — lives in the Next.js app; the Windows and Android builds exist so
that the same code can run without a server install and can write real files to
the device.

## Ways to run it

| Mode | How it starts | Database | Who talks to it |
| --- | --- | --- | --- |
| **Server** | `next start` (or any process manager) | PostgreSQL via `DATABASE_URL` | Browsers, and Android phones on the network |
| **Development** | `next dev` | Embedded PGlite, or PostgreSQL if configured | Your browser |
| **Windows desktop** | Electron starts the bundled standalone server on a free loopback port | Embedded PGlite in `%APPDATA%/Ghost Music/data` | Its own window, plus a phone when phone mode is on |
| **Android** | Native shell loads a server origin in a WebView | None — it is a client | The desktop app or a hosted deployment |

`src/db/index.ts` picks the driver once per process: a `pg` Pool when
`DATABASE_URL` is set, otherwise PGlite (PostgreSQL compiled to WebAssembly)
pointing at `.ghost-library/`. PGlite is real PostgreSQL, so the Drizzle schema,
the SQL, and the transactions are identical in both modes — the embedded schema in
`src/db/embedded-schema.ts` only exists because the packaged builds need to create
their own tables on first launch. `GET /api/health` reports which mode is live.

## HTTP surface

| Route | Purpose |
| --- | --- |
| `GET /api/health` | Liveness and database mode; the desktop app and the Android shell both gate on this |
| `GET /api/library` | Tracks, playlists, likes, history, and downloaded tracks for one listener |
| `POST /api/library` | `like`, `history`, `create-playlist`, `add-to-playlist`, `remove-from-playlist`, `delete-playlist` |
| `GET /api/search` | Searches the local catalog first, then YouTube |
| `POST /api/import` | Imports a public YouTube video or playlist (up to 30 tracks) into a new playlist |
| `POST /api/downloads` | Starts an extraction job |
| `GET /api/downloads` | The listener's recent jobs and their progress |
| `GET /api/downloads/[id]/file` | Streams the finished MP3 as an attachment |
| `GET /api/stream/[id]` | Audio for playback: a local file for Ghost Sessions, a proxied range request for YouTube |
| `GET /api/playlists/[id]` | Public playlist view used by share links |

Identity is a random UUID the client stores and sends as `x-ghost-listener`;
`listenerFrom()` rejects anything that is not a well-formed UUID. **This is not
authentication.** It scopes data to a browser, not to a person, which is why share
links are described as public. Adding real accounts is a deliberate future step.

## Data model

PostgreSQL tables defined in `src/db/schema.ts`:

| Table | Holds |
| --- | --- |
| `ghost_listeners` | One row per browser identity; triggers starter playlists |
| `ghost_tracks` | Catalog plus imported YouTube tracks (`source`, `source_url`, `audio_path`) |
| `ghost_collections` | Likes, history, and download records, keyed `(listener, track, kind)` |
| `ghost_playlists` / `ghost_playlist_tracks` | Playlists and their ordered contents |
| `ghost_downloads` | Extraction jobs: status, progress, byte counts, output path, metadata |

The eight Ghost Sessions instrumentals and six mood mixes are seeded from
`src/lib/catalog.ts`. `ensureCatalog()` inserts the tracks once per process; the
first library request then creates three starter playlists for that listener.

## The download pipeline

1. `POST /api/downloads` checks the track exists, that the track is under 30
   minutes, and — for YouTube sources — that the client sent an explicit
   permission confirmation.
2. Reuses a `ready` job if its file is still on disk, or a job that is still
   running. Concurrent jobs are capped at 2 per listener and 8 per process.
3. Ghost Sessions originals skip extraction entirely: the bundled MP3 is the
   download.
4. For YouTube sources, yt-dlp runs with `-x --audio-format mp3 --embed-metadata
   --embed-thumbnail`, a 100 MB `--max-filesize`, and a 4-minute kill timer. Its
   progress template (`ghost:<bytes>:<total>:<%>`) is parsed on stdout and written
   to `ghost_downloads` at most twice a second.
5. On success the file is stored in `.ghost-downloads/` and the job becomes
   `ready`. On failure the job records a specific reason — restricted, too large,
   timed out — instead of a generic error.

Files are streamed with real HTTP range support in `src/lib/audio-file.ts`, so the
same helper serves playback, seeking, and download.

## Playback, offline, and the service worker

`src/hooks/use-player.ts` owns the queue, shuffle, repeat, and volume, and feeds a
Web Audio `AnalyserNode`. The interface reads real frequency data from it — there
is no simulated progress or invented beat detection.

`public/sw.js` caches the app shell for navigations and `_next/static` assets, and
serves `/audio/*` and `/api/stream/*` with its own range handling from the
`ghost-audio-v1` cache. A downloaded track is written into that cache by the
client, which is what makes offline playback and seeking work with no network.
`/api/*` routes other than streaming are never cached.

## Windows runtime

`desktop/main.js` is the whole native layer:

1. Pick a free loopback port.
2. Start the bundled server (`resources/app`) with the app data directory set, as a
   child process using `ELECTRON_RUN_AS_NODE`.
3. Poll `/api/health` until it answers, showing `desktop/splash.html` meanwhile.
4. Load `http://127.0.0.1:<port>` in a sandboxed `BrowserWindow`.

Because the renderer is a real browser origin rather than a `file://` page, service
workers, range requests, native downloads, and the analyser all behave the same as
on the web. The menu can rebind the server to `0.0.0.0:43110` for phone mode, print
the address, or open `%APPDATA%/Ghost Music/logs/server.log`.

`scripts/build-desktop.mjs` builds Next in standalone mode and stages an explicit
allow-list into `.desktop-build` (the server, its traced dependencies, `public/`,
`.next/static`) — an allow-list rather than a copy of the build output, so dev
data can never ship inside the installer. electron-builder packages that as
`resources/app`.

## Android runtime

`android/app/src/main/java/com/ghostmusic/app/MainActivity.java` asks for a server
address on first launch, verifies `/api/health`, and stores it. The WebView runs the
same client with DOM storage, cookies, service workers, and
`mediaPlaybackRequiresUserGesture=false` enabled.

A WebView cannot follow a `blob:` download, so the shell exposes
`GhostMusic.saveFile(name, base64Chunk, append)` and `GhostMusic.finishFile()`.
`src/lib/device.ts` streams the finished MP3 to it in 384 KB chunks; native code
appends them to `Music/Ghost Music` through MediaStore (API 29+) or the app's
external music folder on older releases. Outside Android the bridge is absent and
the ordinary browser download runs instead.

## Paths and environment

| Variable | Default | Used for |
| --- | --- | --- |
| `DATABASE_URL` | unset | PostgreSQL; unset selects the embedded library |
| `GHOST_DATA_DIR` | current working directory | Root for `.ghost-library/` and `.ghost-downloads/` |
| `GHOST_YTDLP_PATH` / `GHOST_FFMPEG_PATH` | `node_modules/…` | Override the bundled tools (e.g. a system FFmpeg) |
| `PORT` / `HOSTNAME` | `3000` / `localhost` | The Next.js server |

The packaged desktop app sets `GHOST_DATA_DIR` to `%APPDATA%/Ghost Music/data`
because its install directory is read-only.

## Deliberate limits

- No account authentication; libraries belong to a browser identity.
- No rate limiting on search or extraction. Add abuse controls before exposing a
  public deployment.
- Extraction jobs live in the process that started them; a restart loses in-flight
  work. A durable queue is the next step for scale.
- Downloads never expire on their own. A retention policy is the operator's job.
- 30-minute duration cap, 100 MB source cap, 30-track import cap.
