# Troubleshooting

## Windows

### "Windows protected your PC" when I run the installer

The installers are not code-signed, so SmartScreen flags them until the file earns
download reputation. Choose **More info → Run anyway**. To remove the warning
entirely, sign the build with your own certificate by setting `CSC_LINK` and
`CSC_KEY_PASSWORD` before `npm run dist:desktop`.

### The portable build takes a minute to open the first time

That is expected: it unpacks itself to a temporary folder, and antivirus scans the
~170 MB payload on the first launch. Later launches are quicker. If you will use it
regularly, prefer the installer.

### The app opens to a blank window or an error page

Open **Ghost Music → Open server log** and read the tail. The usual causes are:

- another process is already bound to a port the server wanted (rare — a free port
  is chosen automatically),
- the app data folder is not writable (check `%APPDATA%\Ghost Music`),
- antivirus is holding a bundled file. Add an exclusion for the install folder and
  try again.

### Where does the desktop app keep its data?

| What | Where |
| --- | --- |
| Embedded library | `%APPDATA%\Ghost Music\data\.ghost-library` |
| Downloaded audio | `%APPDATA%\Ghost Music\data\.ghost-downloads` |
| Server log | `%APPDATA%\Ghost Music\logs\server.log` |
| Its browser profile (session, caches) | `%APPDATA%\Ghost Music` |

Deleting the `data` folder resets the library to a fresh, seeded state. Note that
another unrelated app on this machine also uses a folder named `Ghost Music`; this
app only ever touches `data`, `logs`, and its own profile inside it.

## Connecting the phone

### The Android app says it cannot reach the server

1. On the desktop app, open **Ghost Music → Connect a phone on this network**. It
   rebinds the server and shows an address like `192.168.1.20:43110`.
2. Type that address into the phone. The desktop **View → Show server address**
   entry prints the current origin if you lost the dialog.
3. Both devices must be on the same Wi-Fi, and Windows Firewall must allow the
   connection. Approve the network prompt the first time, or allow Node.js on
   private networks in Windows Defender Firewall.
4. Many guest and public Wi-Fi networks isolate clients from each other
   (AP isolation). Use a home network, a phone hotspot, or a hosted deployment.
5. Phone mode has **no authentication**. Anyone on that network who knows the
   address can open your library. Turn it on only when you need it; the app returns
   to loopback-only on the next launch.

## Android

### The APK will not install

Enable installation from unknown sources for your browser or file manager, and make
sure the device runs Android 8.0 or newer (minSdk 26). If a previous copy of the app
is installed with a different signing key, uninstall it first — Android refuses
updates signed with a different key.

### Downloads do not appear in my music app

Ghost Music writes to `Music/Ghost Music`. Some music players cache their library;
rescan or reopen the app. On Android 10+ the file is written through MediaStore, so
it should appear without a reboot.

### Playback does not start

The Android shell is a client: it plays through the server it is connected to. If
the server went to sleep, or you switched networks, reconnect from **Server
settings** in the overflow menu.

## Extraction and playback

### "YouTube is limiting playlist imports right now" or a 403

YouTube throttles or blocks datacentre and shared IPs, and changes its internals
regularly. The bundled yt-dlp is a snapshot from build time; update it when
extraction starts failing:

```bash
npx youtube-dl-exec --update-to nightly
npm install youtube-dl-exec@latest ffmpeg-static@latest
```

Then rebuild the desktop payload (`npm run dist:desktop`). Restricted, private, and
region-blocked tracks will always fail — the app reports that instead of inventing
a result.

### "The extraction service could not start"

FFmpeg or yt-dlp is missing. Both are installed by `npm install` and bundled into
the packaged builds; if you deleted `node_modules`, reinstall. To use tools you
manage yourself, set `GHOST_YTDLP_PATH` and `GHOST_FFMPEG_PATH`.

### A download fails only for long tracks

Downloads are capped at 30 minutes and 100 MB, and at 2 concurrent jobs per
listener. The Ghost Sessions originals always download because they ship with the
app.

### Seeking or playback fails only for YouTube tracks

Stream URLs are resolved on demand and cached for 15 minutes; they expire after
that, which is why the app re-resolves them. If playback stops mid-track after a
long pause, press play again.

### The interface stopped reacting to the music

The analyser needs the same origin and a user gesture. If audio is muted at the OS
level, or the tab was restored without a gesture, press play once. Reduced-motion
preferences also disable decorative movement by design.

### The app shows an old version after an update

The service worker serves the shell from cache. Do one hard reload
(`Ctrl + Shift + R`), or use the app's refresh action; the cache is versioned, so a
normal reload is usually enough.

## Build and development

### `npm run build` fails on types

Run `npx next typegen` first — the route types Next.js generates are what `tsc`
needs to see.

### The database is locked or queries fail

Two servers are probably sharing one data directory. Stop the other process, or
give each one its own `GHOST_DATA_DIR`. PGlite allows a single writer.

### `npm run lint` reports pre-existing errors

The lint script is not part of the build; `npm run build` and `npx tsc --noEmit`
are the gates CI should use.

### Regenerating media

```bash
npm run assets
```

Both generators skip files that already exist. Delete `public/audio/*.mp3` or
`public/images/*.png` first to force a rebuild.
