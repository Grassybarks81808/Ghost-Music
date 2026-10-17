import { chromium } from 'playwright';
import assert from 'node:assert/strict';
import { mkdir, stat, readFile } from 'node:fs/promises';

const base = process.env.TEST_BASE_URL || 'http://127.0.0.1:3000';
await mkdir('artifacts', { recursive: true });
const browser = await chromium.launch({ headless: true, args: ['--no-sandbox', '--autoplay-policy=no-user-gesture-required'] });
const context = await browser.newContext({ viewport: { width: 1440, height: 900 }, acceptDownloads: true });
const page = await context.newPage();
const errors = [];
page.on('pageerror', error => errors.push(error.message));
try {
  await page.goto(base, { waitUntil: 'networkidle' });
  await page.locator('.ghost-intro').waitFor({ state: 'visible' });
  const introStarted = Date.now();
  await page.waitForTimeout(3100);
  await page.screenshot({ path: 'artifacts/startup.png' });
  await page.locator('.ghost-intro').waitFor({ state: 'hidden', timeout: 12000 });
  assert(Date.now() - introStarted >= 8000, 'The startup ritual should last approximately ten seconds');
  await page.evaluate(() => document.fonts.ready);
  await page.screenshot({ path: 'artifacts/home-desktop.png' });
  assert(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth), 'Desktop should not overflow');
  assert.equal(await page.locator('.mix-card').count(), 6);
  console.log('PASS: 10-second startup, desktop feed, six custom playlist covers');

  await page.getByRole('button', { name: 'Start listening', exact: true }).click();
  await page.waitForFunction(() => Number(document.querySelector('.ghost-range')?.value) > 1, { timeout: 15000 });
  const energy = await page.evaluate(() => Number(getComputedStyle(document.documentElement).getPropertyValue('--beat')));
  assert(energy > 0, 'Audio analyser should produce a real beat signal');
  await page.locator('.ghost-range').click({ position: { x: 180, y: 2 } });
  await page.waitForFunction(() => Number(document.querySelector('.ghost-range')?.value) > 20);
  await page.getByRole('button', { name: 'Like current song', exact: true }).click();
  await page.getByRole('button', { name: 'Next track', exact: true }).click();
  await page.waitForFunction(() => document.querySelector('.playing-copy>button')?.textContent === 'Weightless');
  await page.getByRole('button', { name: 'Open play queue', exact: true }).click();
  await page.locator('.queue-track').filter({ hasText: 'Golden' }).click();
  await page.getByRole('button', { name: 'Close queue', exact: true }).click();
  await page.waitForFunction(() => document.querySelector('.playing-copy>button')?.textContent === 'Golden');
  console.log('PASS: playback, seek, analyser, likes, next track, play queue');

  const search = page.getByRole('textbox', { name: 'Search songs, artists, or a YouTube link' });
  await search.fill('neon');
  await search.press('Enter');
  await page.locator('.search-loading').waitFor({ state: 'hidden' });
  await page.getByRole('button', { name: 'Play Neon Dreams', exact: true }).waitFor();
  console.log('PASS: search with persisted catalog');

  await page.getByRole('button', { name: 'Create playlist', exact: true }).click();
  const playlistName = `Browser test orbit ${Date.now()}`;
  await page.getByLabel('Playlist name', { exact: true }).fill(playlistName);
  await page.locator('.modal').getByRole('button', { name: 'Create playlist', exact: true }).click();
  await page.locator('.modal').waitFor({ state: 'hidden' });
  await page.getByRole('heading', { name: playlistName, exact: true }).waitFor();
  await page.locator('.primary-nav').getByRole('button', { name: 'Home', exact: true }).click();
  await page.getByRole('button', { name: 'More options for Midnight Drive', exact: true }).click();
  await page.getByLabel('Add to your playlist', { exact: true }).selectOption({ label: playlistName });
  await page.locator('.modal').waitFor({ state: 'hidden' });
  await page.locator('.sidebar-playlist').filter({ hasText: playlistName }).click();
  await page.getByRole('button', { name: 'Play Midnight Drive', exact: true }).waitFor();
  await page.getByRole('button', { name: 'Share playlist', exact: true }).click();
  const shareLink = await page.getByLabel('Your shareable link').inputValue();
  const playlistId = new URL(shareLink).searchParams.get('playlist');
  const sharedResponse = await page.request.get(`${base}/api/playlists/${playlistId}`);
  const shared = await sharedResponse.json();
  assert.equal(shared.playlist.name, playlistName);
  assert.equal(shared.tracks[0].id, 'midnight-drive');
  await page.getByRole('button', { name: 'Close dialog', exact: true }).click();
  console.log('PASS: create playlist, add song, persistent shareable playlist API');

  const downloadPromise = page.waitForEvent('download', { timeout: 30000 });
  await page.getByRole('button', { name: 'Download current track', exact: true }).click();
  const download = await downloadPromise;
  assert(download.suggestedFilename().endsWith('Golden.mp3'));
  await download.saveAs('artifacts/verified-download.mp3');
  assert((await stat('artifacts/verified-download.mp3')).size > 500000);
  const mp3 = await readFile('artifacts/verified-download.mp3');
  assert.equal(mp3.subarray(0, 3).toString(), 'ID3');
  await page.getByRole('heading', { name: 'Yours. Even offline.', exact: true }).waitFor();
  const cached = await page.evaluate(async () => { const cache = await caches.open('ghost-audio-v1'); return !!(await cache.match('/audio/golden.mp3')); });
  assert(cached, 'Downloaded audio must be stored in browser offline cache');
  await page.screenshot({ path: 'artifacts/download-complete.png' });
  await page.getByRole('button', { name: 'Hide download progress', exact: true }).click();
  console.log('PASS: full MP3 download, embedded ID3 metadata, byte progress, offline audio cache');

  const listener = await page.evaluate(() => localStorage.getItem('ghost-listener'));
  const libraryResponse = await page.request.get(`${base}/api/library`, { headers: { 'x-ghost-listener': listener } });
  const library = await libraryResponse.json();
  assert(library.liked.includes('midnight-drive'));
  assert(library.downloaded.includes('golden'));
  assert(library.history.includes('golden'));
  const jobsResponse = await page.request.get(`${base}/api/downloads`, { headers: { 'x-ghost-listener': listener } });
  const jobs = await jobsResponse.json();
  const wrongOwner = await page.request.get(`${base}/api/downloads/${jobs.jobs[0].id}/file`, { headers: { 'x-ghost-listener': '11111111-1111-4111-8111-111111111111' } });
  assert.equal(wrongOwner.status(), 404);
  const ranged = await page.request.get(`${base}/api/stream/midnight-drive`, { headers: { Range: 'bytes=0-4095' } });
  assert.equal(ranged.status(), 206);
  assert.equal((await ranged.body()).length, 4096);
  console.log('PASS: persisted likes/history/downloads, download ownership, audio range requests');

  await page.locator('.primary-nav').getByRole('button', { name: 'Home', exact: true }).click();
  await page.getByRole('button', { name: 'Import playlist', exact: true }).click();
  await page.getByLabel('YouTube song or playlist link').fill('https://example.com/not-youtube');
  await page.getByRole('button', { name: 'Import to Ghost Music', exact: true }).click();
  await page.locator('.form-error').filter({ hasText: 'Only YouTube' }).waitFor();
  await page.getByRole('button', { name: 'Close dialog', exact: true }).click();
  await page.getByRole('button', { name: 'Pause', exact: true }).click();
  await page.reload({ waitUntil: 'networkidle' });
  await page.locator('.sidebar-playlist').filter({ hasText: playlistName }).waitFor();
  console.log('PASS: source validation and playlist persistence after reload');

  await context.setOffline(true);
  await page.reload({ waitUntil: 'domcontentloaded' });
  await page.locator('.primary-nav').getByRole('button', { name: 'Home', exact: true }).waitFor();
  await page.locator('nav[aria-label="Music collection"]').getByRole('button', { name: /Downloads/ }).click();
  await page.getByRole('button', { name: 'Play Golden', exact: true }).first().click();
  await page.waitForFunction(() => Number(document.querySelector('.ghost-range')?.value) > 1, { timeout: 15000 });
  await page.getByRole('button', { name: 'Pause', exact: true }).click();
  await context.setOffline(false);
  console.log('PASS: complete offline page load and playback of downloaded MP3');

  const cleanListener = crypto.randomUUID();
  const cleanHeaders = { 'x-ghost-listener': cleanListener, 'Content-Type': 'application/json' };
  const seededResponse = await page.request.get(`${base}/api/library`, { headers: cleanHeaders });
  const seededLibrary = await seededResponse.json();
  assert.equal(seededLibrary.playlists.length, 3);
  for (const playlist of seededLibrary.playlists) {
    const response = await page.request.post(`${base}/api/library`, { headers: cleanHeaders, data: { action: 'delete-playlist', playlistId: playlist.id } });
    assert.equal(response.status(), 200);
  }
  const emptyResponse = await page.request.get(`${base}/api/library`, { headers: cleanHeaders });
  assert.equal((await emptyResponse.json()).playlists.length, 0, 'Deleted starter playlists must not reappear');
  console.log('PASS: intentionally empty libraries stay empty after all playlists are deleted');

  const mobileContext = await browser.newContext({ viewport: { width: 390, height: 844 }, isMobile: true, hasTouch: true });
  await mobileContext.addInitScript(() => sessionStorage.setItem('ghost-intro-seen', '1'));
  const mobile = await mobileContext.newPage();
  mobile.on('pageerror', error => errors.push(error.message));
  await mobile.goto(base, { waitUntil: 'networkidle' });
  await mobile.evaluate(() => document.fonts.ready);
  assert(await mobile.evaluate(() => document.documentElement.scrollWidth <= innerWidth), 'Mobile should not overflow');
  await mobile.screenshot({ path: 'artifacts/home-mobile.png' });
  await mobile.getByRole('button', { name: 'Open navigation', exact: true }).click();
  await mobile.locator('.primary-nav').getByRole('button', { name: 'Your library', exact: true }).click();
  await mobile.getByRole('heading', { name: 'Your library', exact: true }).waitFor();
  assert.equal(await mobile.locator('.sidebar.sidebar-open').count(), 0);
  await mobile.screenshot({ path: 'artifacts/library-mobile.png' });
  await mobileContext.close();
  assert.deepEqual(errors, [], 'There should be no uncaught browser errors');
  console.log('PASS: mobile feed, responsive navigation, library layout, no browser exceptions');
  console.log('ALL GHOST MUSIC END-TO-END CHECKS PASSED');
} catch (error) {
  await page.screenshot({ path: 'artifacts/test-failure.png', fullPage: true });
  console.error('BROWSER ERRORS:', errors);
  throw error;
} finally {
  await browser.close();
}
