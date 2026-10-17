const STATIC_CACHE = 'ghost-shell-v1';
const AUDIO_CACHE = 'ghost-audio-v1';
self.addEventListener('install', event => { self.skipWaiting(); event.waitUntil(caches.open(STATIC_CACHE).then(cache => cache.add('/')).catch(() => {})); });
self.addEventListener('activate', event => { event.waitUntil(self.clients.claim()); });
self.addEventListener('fetch', event => {
  const request = event.request;
  const url = new URL(request.url);
  if (request.method !== 'GET' || url.origin !== self.location.origin) return;
  const audio = url.pathname.startsWith('/audio/') || url.pathname.startsWith('/api/stream/');
  if (audio) {
    event.respondWith(caches.open(AUDIO_CACHE).then(async cache => {
      const cached = await cache.match(url.pathname);
      if (!cached) return fetch(request);
      const range = request.headers.get('range');
      if (!range) return cached;
      const bytes = await cached.arrayBuffer();
      const match = /^bytes=(\d*)-(\d*)$/.exec(range);
      if (!match) return new Response(null, { status: 416 });
      const start = match[1] ? Number(match[1]) : Math.max(0, bytes.byteLength - Number(match[2]));
      const end = match[1] && match[2] ? Math.min(Number(match[2]), bytes.byteLength - 1) : bytes.byteLength - 1;
      if (start > end || start >= bytes.byteLength) return new Response(null, { status: 416, headers: { 'Content-Range': `bytes */${bytes.byteLength}` } });
      return new Response(bytes.slice(start, end + 1), { status: 206, headers: { 'Content-Type': 'audio/mpeg', 'Accept-Ranges': 'bytes', 'Content-Range': `bytes ${start}-${end}/${bytes.byteLength}`, 'Content-Length': String(end - start + 1) } });
    }));
    return;
  }
  if (url.pathname.startsWith('/api/')) return;
  if (request.mode === 'navigate') {
    event.respondWith(fetch(request).then(response => { if(response.ok) { const copy=response.clone(); void caches.open(STATIC_CACHE).then(cache => cache.put('/', copy)); } return response; }).catch(() => caches.match('/')));
    return;
  }
  if (url.pathname.startsWith('/_next/static/') || url.pathname.startsWith('/images/')) {
    event.respondWith(caches.open(STATIC_CACHE).then(async cache => { const cached=await cache.match(request); if(cached) return cached; const response=await fetch(request); if(response.ok) await cache.put(request,response.clone()); return response; }));
  }
});
