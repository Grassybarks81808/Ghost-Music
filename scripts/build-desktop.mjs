/**
 * Builds the Next.js server in standalone mode and stages everything the
 * Electron app needs into .desktop-build. electron-builder then ships that
 * folder as resources/app.
 */
import { execFileSync } from 'node:child_process';
import { cpSync, existsSync, mkdirSync, readdirSync, rmSync, statSync, copyFileSync } from 'node:fs';
import path from 'node:path';

const root = process.cwd();
const nextBin = path.join(root, 'node_modules', 'next', 'dist', 'bin', 'next');
const stage = path.join(root, '.desktop-build');

const required = [
  'public/audio/midnight-drive.mp3',
  'public/images/after-hours.png',
  'public/icons/ghost-512.png',
];
for (const file of required) {
  if (!existsSync(path.join(root, file))) {
    console.error(`Missing ${file}. Run "node scripts/generate-art.mjs" and "node scripts/generate-audio.mjs" first.`);
    process.exit(1);
  }
}

console.log('› next build (standalone)');
execFileSync(process.execPath, [nextBin, 'build'], {
  cwd: root,
  stdio: 'inherit',
  env: { ...process.env, GHOST_STANDALONE: '1', NEXT_TELEMETRY_DISABLED: '1' },
});

const standalone = path.join(root, '.next', 'standalone');
if (!existsSync(path.join(standalone, 'server.js'))) {
  console.error('Standalone output was not produced. Check that next.config.ts sets output: "standalone" when GHOST_STANDALONE=1.');
  process.exit(1);
}

console.log('› staging app payload');
rmSync(stage, { recursive: true, force: true });
cpSync(standalone, stage, { recursive: true });
cpSync(path.join(root, '.next', 'static'), path.join(stage, '.next', 'static'), { recursive: true });
cpSync(path.join(root, 'public'), path.join(stage, 'public'), { recursive: true });

// Next copies sibling project folders into standalone. Keep only what the
// server actually serves, so installers stay lean and no local data ships.
const KEEP = new Set(['server.js', 'package.json', '.next', 'node_modules', 'public']);
for (const entry of readdirSync(stage)) {
  if (!KEEP.has(entry)) rmSync(path.join(stage, entry), { recursive: true, force: true });
}
rmSync(path.join(stage, '.next', 'cache'), { recursive: true, force: true });

const checks = [
  'server.js',
  '.next/static',
  'public/audio/midnight-drive.mp3',
  'public/images/ghost-hero.png',
  'node_modules/@electric-sql/pglite',
  'node_modules/ffmpeg-static',
  'node_modules/youtube-dl-exec/bin',
];
let failed = false;
for (const file of checks) {
  const target = path.join(stage, file);
  const ok = existsSync(target);
  console.log(`  ${ok ? '✓' : '✗'} ${file}`);
  if (!ok) failed = true;
}
if (failed) {
  console.error('The staged app is incomplete; packaged builds would fail at runtime.');
  process.exit(1);
}

const ytdlp = path.join(stage, 'node_modules', 'youtube-dl-exec', 'bin', process.platform === 'win32' ? 'yt-dlp.exe' : 'yt-dlp');
const ffmpeg = path.join(stage, 'node_modules', 'ffmpeg-static', process.platform === 'win32' ? 'ffmpeg.exe' : 'ffmpeg');
for (const binary of [ytdlp, ffmpeg]) {
  if (existsSync(binary)) {
    const size = statSync(binary).size;
    console.log(`  ✓ ${path.relative(stage, binary)} (${(size / 1024 / 1024).toFixed(1)} MB)`);
  } else {
    console.log(`  ✗ ${path.relative(stage, binary)} (missing — YouTube import and search will not work)`);
    failed = true;
  }
}
if (failed) process.exit(1);

// Window/taskbar icon for the packaged shell.
copyFileSync(path.join(root, 'assets', 'icon-1024.png'), path.join(root, 'desktop', 'icon.png'));
console.log('✓ staging complete: .desktop-build');
