# Building and releasing Ghost Music

Everything in the release is generated from this repository. The normal web
deployment is unchanged: `npm run build` + `npm start` against PostgreSQL works
exactly as it always did.

## What gets produced

| Artifact | Command | Output |
| --- | --- | --- |
| Windows installer | `npm run dist:desktop` | `dist/GhostMusic-1.0.0-x64.exe` |
| Windows portable | `npm run dist:desktop` | `dist/GhostMusic-1.0.0-portable.exe` |
| Android APK | `npm run build:android` | `dist/GhostMusic-1.0.0.apk` |

Both Windows artifacts contain the whole app: the Next.js production server,
FFmpeg, yt-dlp, the cover art, and the eight Ghost Sessions MP3s. Nothing is
downloaded at first run, and no database server is needed — the packaged app uses
its embedded PGlite library.

## Prerequisites

| For | You need |
| --- | --- |
| Everything | Node.js 22+ and npm |
| Web/server | PostgreSQL 14+ (optional; otherwise the embedded library is used) |
| Windows builds | Nothing beyond Node — electron-builder downloads its own toolchain into its cache |
| Android builds | JDK 21, Android SDK 35, Gradle 8.9 — `bash scripts/install-android-toolchain.sh` installs all three into `~/tools` |
| Verification | Chromium for the Playwright suites: `npx playwright install chromium` |

Install dependencies and generate the media once:

```bash
npm install
npm run assets     # six covers, hero image, PWA/Windows/Android icons, eight MP3s
```

`npm run assets` is idempotent: both generators skip files that already exist. The
generated media is committed, so a fresh clone can build without regenerating it.

## Windows desktop app

```bash
npm run dist:desktop
```

That runs `scripts/build-desktop.mjs` and then electron-builder:

1. `next build` with `GHOST_STANDALONE=1`, producing the standalone server.
2. Staging into `.desktop-build/` from an **explicit allow-list** — the standalone
   server and its traced dependencies, `public/`, and `.next/static`. An allow-list
   rather than a copy of the build output, so development data (test databases,
   screenshots, logs) can never be packaged into the installer.
3. electron-builder packages `.desktop-build` as `resources/app` inside the Electron
   shell in `desktop/`, and writes the NSIS installer plus the portable executable.

How the packaged app runs:

- `desktop/main.js` picks a free loopback port, starts the bundled server as a child
  Node process (`ELECTRON_RUN_AS_NODE`), waits for `/api/health`, then loads that
  origin in a sandboxed `BrowserWindow`. Service workers, range requests, native
  downloads, and the Web Audio analyser therefore behave exactly as they do on the web.
- With no `DATABASE_URL`, the server opens an embedded PostgreSQL database (PGlite)
  in `%APPDATA%/Ghost Music/data`. A `DATABASE_URL` in the environment still wins.
- Downloads go to `%APPDATA%/Ghost Music/data/.ghost-downloads`; logs to
  `%APPDATA%/Ghost Music/logs/server.log` (**Ghost Music → Open server log**).
- **Connect a phone on this network** restarts the server on `0.0.0.0:43110` (or the
  next free port) and shows the address for the Android app. There is no
  authentication on it, so use it only on a network you trust; the next launch
  returns to loopback-only.

### Verifying a Windows build

```bash
"dist/win-unpacked/Ghost Music.exe" --remote-debugging-port=9222
node scripts/verify-desktop.mjs
```

This attaches to the running app over the Chrome DevTools protocol and checks window
rendering, artwork, real audio playback with live analyser output, and a completed
download. To check the installer itself, run the produced `dist/*.exe`, and confirm
that the app answers `GET /api/health` with `{"ok":true,"database":"embedded"}`.

### Signing

The installers are unsigned by default, so SmartScreen warns on first run. To sign:

```bash
export CSC_LINK=/path/to/certificate.pfx
export CSC_KEY_PASSWORD=...
npm run dist:desktop
```

## Android app

```bash
bash scripts/install-android-toolchain.sh   # once: JDK 21, Android SDK 35, Gradle 8.9
npm run build:android
```

The first Android build creates `android/ghost-release.jks` and
`android/keystore.properties`, both git-ignored. **Keep that keystore** — future
releases must be signed with the same key to install over the previous one, and
losing it means users must uninstall first.

Ghost Music for Android is a client. The library, extraction pipeline, and database
live on a Ghost Music server: the desktop app with phone mode enabled, or a hosted
deployment of this repository. The shell:

- asks for the server address on first launch and verifies it against `/api/health`
  before saving it; **Server settings** clears it when you change networks;
- enables DOM storage, cookies, and service workers so downloaded tracks keep
  playing offline inside the app;
- sets `mediaPlaybackRequiresUserGesture=false`, so playback starts from the app's
  own buttons;
- exposes a `GhostMusic.saveFile` bridge, because a WebView cannot follow a `blob:`
  download. `src/lib/device.ts` streams the finished MP3 to native code in 384 KB
  chunks, which writes it to `Music/Ghost Music` through MediaStore (API 29+) or the
  app's external music folder on older releases;
- allows plain `http://` for local-network servers, and opens any other host in the
  system browser instead of the WebView.

### Verifying an Android build

```bash
node scripts/verify-android-bridge.mjs
apksigner verify --print-certs dist/GhostMusic-1.0.0.apk
aapt2 dump badging dist/GhostMusic-1.0.0.apk
```

The first script injects a stub bridge into Chromium, downloads a track, and asserts
the received bytes are a complete ID3-tagged MP3 and that the browser fallback never
fired. The other two confirm the signature and the package metadata
(`com.ghostmusic.app`, minSdk 26, targetSdk 34, five launcher icons).

Installing on a physical device is the step that has to happen on your machine:

```bash
adb install -r dist/GhostMusic-1.0.0.apk
```

## What was verified for 1.0.0

| Check | Result |
| --- | --- |
| `npx next typegen` + `npx tsc --noEmit` | Clean |
| `npm run build` + `npm start` (server path) | Clean; PostgreSQL path unchanged |
| `node scripts/smoke-test.mjs` against the production build | All ten groups pass |
| `node scripts/verify-desktop.mjs` against the packaged app | Rendering, artwork, live analyser output, completed download |
| Both Windows artifacts launched | Server started, `/api/health` OK, page and a 3.5 MB MP3 served |
| `node scripts/verify-android-bridge.mjs` | Complete 3.1 MB ID3-tagged MP3 through the bridge; fallback did not fire |
| `apksigner` / `aapt2` on the APK | Valid v2 signature; package, SDK levels, permissions, icons confirmed |
| Bundled FFmpeg and yt-dlp inside the packaged payload | Both execute |

**Not verified here, stated plainly:** installing the APK on a physical device or
emulator (there is no device on this machine), code signing (no certificate), and
the PostgreSQL branch end to end (no PostgreSQL server available locally). The
installers are unsigned, so expect a SmartScreen warning. `npm run lint` reports four
pre-existing warnings unrelated to the packaged builds; the build and type gates are
clean.

## Publishing a release

Tags and assets are what users download, so keep them in step with
`package.json` and `desktop/package.json` (both `1.0.0`).

```bash
# 1. Build everything, then checksum it for the release page
npm run dist:desktop
npm run build:android
cd dist && sha256sum GhostMusic-1.0.0-x64.exe GhostMusic-1.0.0-portable.exe \
  GhostMusic-1.0.0.apk > SHA256SUMS.txt && cd ..

# 2. Tag and push the source
git tag -a v1.0.0 -m "Ghost Music 1.0.0"
git push origin main --tags

# 3. Create the release and upload the assets (gh CLI)
gh release create v1.0.0 \
  dist/GhostMusic-1.0.0-x64.exe \
  dist/GhostMusic-1.0.0-portable.exe \
  dist/GhostMusic-1.0.0.apk \
  dist/SHA256SUMS.txt \
  --title "Ghost Music 1.0.0" --notes-file CHANGELOG.md
```

`dist/` is git-ignored on purpose: GitHub rejects files over 100 MB, and both
Windows builds are ~170 MB. They belong in the release, not in the history. Commit
`package.json` version bumps and a `CHANGELOG.md` entry before tagging.

## Regenerating assets

```bash
npm run assets
```

`scripts/generate-art.mjs` draws the six album covers, the hero image, the manifest
icons, a Windows `.ico`, and the Android launcher icons with a dependency-free
PNG/ICO encoder. `scripts/generate-audio.mjs` synthesises the eight instrumentals and
encodes them as tagged MP3s with embedded artwork. Both skip files that already
exist, so delete the target file to force a rebuild.
