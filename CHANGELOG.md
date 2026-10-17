# Changelog

Notable changes to Ghost Music. This project follows [semantic versioning](https://semver.org/).

## 1.0.0 — 2026-10-17

The first release with real installers for both platforms.

### Windows desktop app

- New Electron shell (`desktop/`) that runs the bundled Next.js standalone server
  as a child process on a free loopback port, waits for `/api/health`, and loads it
  in a sandboxed window. Service workers, HTTP range requests, downloads, and the
  Web Audio analyser therefore behave exactly as they do on the web.
- `npm run dist:desktop` builds an NSIS installer and a portable executable, both
  containing the server, FFmpeg, yt-dlp, the artwork, and the eight Ghost Sessions
  MP3s. Nothing is downloaded at first run.
- App menu: **Connect a phone on this network** (rebinds to `0.0.0.0:43110` and shows
  the address), **Show server address**, and **Open server log**.
- Packaged builds store their library, downloads, and logs in
  `%APPDATA%/Ghost Music` instead of the read-only install directory.

### Android app

- Native shell around the same web client: first-run server address prompt verified
  against `/api/health`, remembered afterwards, with **Server settings** to clear it.
- `GhostMusic.saveFile` bridge so a WebView can save downloads — `src/lib/device.ts`
  streams finished MP3s to native code in 384 KB chunks, written to
  `Music/Ghost Music` through MediaStore (API 29+) or the app's music folder on older
  releases.
- WebView configured with DOM storage, cookies, service workers, and
  `mediaPlaybackRequiresUserGesture=false`; plain `http://` allowed for local servers.
- Signed release APK (`apksigner` v2 signature), launcher icons for all five densities.
- `bash scripts/install-android-toolchain.sh` installs JDK 21, Android SDK 35, and
  Gradle 8.9 locally.

### Data and runtime

- **Embedded PostgreSQL.** With no `DATABASE_URL`, the app runs on PGlite (PostgreSQL
  compiled to WebAssembly) in `.ghost-library/`, using the identical Drizzle schema
  and queries. A `DATABASE_URL` still takes priority, so the server deployment is
  unchanged. This is what lets the installers ship without a database server.
- New `src/lib/paths.ts` centralises `GHOST_DATA_DIR`, the downloads directory, and
  the embedded library location.
- Fixed yt-dlp and FFmpeg resolution inside packaged builds, where Windows needs the
  explicit `.exe` suffix.
- `.env.example` documents every supported variable.

### Assets

- `scripts/generate-art.mjs` draws the six album covers, the hero image, the PWA
  icons, a Windows `.ico`, and the Android launcher icons with a dependency-free
  PNG/ICO encoder.
- `scripts/generate-audio.mjs` composes and encodes the eight original Ghost Sessions
  instrumentals as tagged MP3s with embedded artwork — 28 MB committed so a clone
  builds and runs without regenerating media.

### Verification

- `scripts/smoke-test.mjs`: ten browser groups passing against the production build.
- `scripts/verify-desktop.mjs`: attaches to the real packaged app over the Chrome
  DevTools protocol and checks rendering, artwork, live analyser output, and a
  completed download.
- `scripts/verify-android-bridge.mjs`: reassembles a bridged download and asserts a
  valid ID3-tagged MP3 came through while the browser fallback did not fire.
- Documented in [BUILDING.md](BUILDING.md), including the checks that could not run
  here (physical Android device, code signing, PostgreSQL).

### Known limitations

- The Windows installers are **not code-signed**, so SmartScreen warns on first run.
- The APK was validated with `apksigner` and `aapt2` but never installed on a
  physical device or emulator.
- The Android app needs a reachable Ghost Music server; it does not run the
  extraction pipeline locally.
- Libraries are scoped to a random browser identity, not authenticated accounts.
