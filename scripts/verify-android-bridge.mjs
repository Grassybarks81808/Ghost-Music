/**
 * Verifies the Android-only save path: the WebView cannot follow blob: links,
 * so the app hands downloaded bytes to the native GhostMusic bridge in chunks.
 * This runs that exact code path in Chromium with a stub bridge and checks the
 * reassembled file is a real, complete, tagged MP3.
 *
 *   node scripts/verify-android-bridge.mjs   (against TEST_BASE_URL, default :3000)
 */
import { chromium } from 'playwright';
import assert from 'node:assert/strict';

const base = process.env.TEST_BASE_URL || 'http://127.0.0.1:3000';
const browser = await chromium.launch({ headless: true, args: ['--no-sandbox', '--autoplay-policy=no-user-gesture-required'] });
const context = await browser.newContext({ viewport: { width: 1440, height: 900 }, acceptDownloads: true });
await context.addInitScript(() => {
  const chunks = new Map();
  window.__ghostSaved = [];
  window.GhostMusic = {
    saveFile(name, base64Chunk, append) {
      if (!append) chunks.set(name, []);
      chunks.get(name).push(base64Chunk);
      return true;
    },
    finishFile() {
      const [name, parts] = [...chunks.entries()].at(-1);
      window.__ghostSaved.push({ name, base64: parts.join('') });
      return true;
    },
  };
});

const page = await context.newPage();
const errors = [];
page.on('pageerror', error => errors.push(error.message));
await page.addInitScript(() => sessionStorage.setItem('ghost-intro-seen', '1'));
await page.goto(base, { waitUntil: 'networkidle' });
await page.getByRole('button', { name: 'Start listening', exact: true }).click();
await page.waitForFunction(() => Number(document.querySelector('.ghost-range')?.value) > 1, { timeout: 20000 });

let nativeDownloadRan = false;
page.on('download', () => { nativeDownloadRan = true; });

await page.getByRole('button', { name: 'Download current track', exact: true }).click();
await page.getByRole('heading', { name: 'Yours. Even offline.', exact: true }).waitFor({ timeout: 45000 });

const saved = await page.evaluate(() => window.__ghostSaved.map(entry => ({ name: entry.name, bytes: Math.floor((entry.base64.length * 3) / 4) })));
assert.equal(saved.length, 1, `The bridge should receive exactly one file, saw ${JSON.stringify(saved)}`);
assert(saved[0].name.endsWith('.mp3'), `Expected an .mp3 filename, got ${saved[0].name}`);
assert(saved[0].bytes > 500000, `Expected a real song, got ${saved[0].bytes} bytes`);

// Decode the first 3 bytes of the received stream to confirm the ID3 payload.
const header = await page.evaluate(() => {
  const base64 = window.__ghostSaved[0].base64.slice(0, 16);
  return atob(base64).slice(0, 3);
});
assert.equal(header, 'ID3');
assert.equal(nativeDownloadRan, false, 'The browser download fallback should not run when the bridge handles the file');
assert.deepEqual(errors, [], `No page errors expected: ${JSON.stringify(errors)}`);

console.log(`PASS: Android bridge received ${saved[0].name} (${(saved[0].bytes / 1024 / 1024).toFixed(1)} MB, ID3 tagged), browser fallback unused`);
await browser.close();
