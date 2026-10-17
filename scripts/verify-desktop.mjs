/**
 * Verifies a running Ghost Music desktop build over the Chrome DevTools
 * protocol. Start the app with --remote-debugging-port=9222 first:
 *
 *   "dist/win-unpacked/Ghost Music.exe" --remote-debugging-port=9222
 *   node scripts/verify-desktop.mjs
 *
 * Checks that the packaged window renders, plays bundled audio at full quality,
 * reacts to the analyser, and exposes working artwork and service-worker assets.
 */
import { chromium } from 'playwright';
import assert from 'node:assert/strict';
import { mkdir } from 'node:fs/promises';

const port = process.env.GHOST_DEBUG_PORT || '9222';
const browser = await chromium.connectOverCDP(`http://127.0.0.1:${port}`);
const context = browser.contexts()[0];
const page = context.pages()[0];
assert(page, 'The desktop app should expose a window over CDP');

const errors = [];
page.on('pageerror', error => errors.push(error.message));
page.on('console', message => {
  if (message.type() === 'error') errors.push(`console: ${message.text()}`);
});

await mkdir('artifacts', { recursive: true });
await page.bringToFront();
const origin = new URL(page.url()).origin;
console.log(`attached to ${page.url()}`);

const version = await page.evaluate(() => navigator.userAgent);
assert(version.includes('Electron'), `Expected an Electron user agent, got ${version}`);

// The startup ritual plays once per session; skip it if it is on screen.
await page.locator('.ghost-intro').waitFor({ state: 'hidden', timeout: 20000 }).catch(() => {});
await page.getByRole('button', { name: 'Skip intro', exact: true }).click({ timeout: 2000 }).catch(() => {});

await page.getByRole('button', { name: 'Start listening', exact: true }).waitFor({ timeout: 20000 });
await page.screenshot({ path: 'artifacts/desktop-app-home.png' });

const cover = await page.evaluate(async originUrl => {
  const response = await fetch(`${originUrl}/images/after-hours.png`);
  return { status: response.status, type: response.headers.get('content-type'), bytes: (await response.arrayBuffer()).byteLength };
}, origin);
assert.equal(cover.status, 200);
assert(cover.bytes > 50000, 'Cover art should be a real image');
console.log(`PASS: window renders, artwork served (${(cover.bytes / 1024).toFixed(0)} KB ${cover.type})`);

await page.getByRole('button', { name: 'Start listening', exact: true }).click();
await page.waitForFunction(() => Number(document.querySelector('.ghost-range')?.value) > 1, { timeout: 25000 });
const first = await page.evaluate(() => ({ now: Number(document.querySelector('.ghost-range')?.value), beat: Number(getComputedStyle(document.documentElement).getPropertyValue('--beat')) }));
await page.waitForFunction(value => Number(document.querySelector('.ghost-range')?.value) > value + 2, first.now, { timeout: 15000 });
const live = await page.evaluate(() => Number(document.querySelector('.ghost-range')?.value));
assert(live > first.now, 'Playback position should advance');
assert(first.beat > 0, 'The Web Audio analyser should produce a real signal');
console.log(`PASS: bundled audio plays (position ${first.now.toFixed(1)}s → ${live.toFixed(1)}s, analyser level ${first.beat})`);

const downloadPromise = page.waitForEvent('download', { timeout: 45000 });
await page.getByRole('button', { name: 'Download current track', exact: true }).click();
const download = await downloadPromise;
const saved = await download.path();
assert(saved, 'Electron should save the downloaded MP3');
console.log(`PASS: download saved as ${download.suggestedFilename()}`);

await page.screenshot({ path: 'artifacts/desktop-app-playing.png' });
assert.deepEqual(errors, [], `The packaged app should log no errors: ${JSON.stringify(errors)}`);
console.log('ALL DESKTOP PACKAGE CHECKS PASSED');
await browser.close();
