/*
 * sw.js - the service worker, which does exactly two jobs:
 *
 *   1. Makes Heptad work with no network at all. An instrument you can only
 *      play when you have signal is a worse instrument.
 *   2. Satisfies the browser's "is this a real app?" check, which is part of
 *      what makes Chrome and Edge offer to install it.
 *
 * It is not registered when you open index.html straight off disk - service
 * workers don't exist on file:// - so double-clicking the file still behaves
 * exactly as it did before this file existed.
 */

// Bump this when the cached files change. The old cache is deleted on activate,
// which is the entire cache-invalidation strategy and all this needs.
const CACHE = 'heptad-v2';

const FILES = [
  './',
  './index.html',
  './manifest.json',
  './src/theory.js',
  './src/app.js',
  './src/looper.js',
  './src/arp.js',
  './icons/icon-192.png',
  './icons/icon-512.png',
];

self.addEventListener('install', (e) => {
  e.waitUntil(
    caches.open(CACHE)
      .then((cache) => cache.addAll(FILES))
      .then(() => self.skipWaiting()) // don't wait for old tabs to close
  );
});

self.addEventListener('activate', (e) => {
  e.waitUntil(
    caches.keys()
      .then((names) => Promise.all(
        names.filter((n) => n !== CACHE).map((n) => caches.delete(n))
      ))
      .then(() => self.clients.claim())
  );
});

/*
 * Stale-while-revalidate: answer instantly from the cache, then quietly fetch a
 * fresh copy for next time.
 *
 * The tradeoff is that after a deploy you are one launch behind - you get the
 * new version the second time you open it. For an instrument that is the right
 * way round: it always starts instantly and always works on a plane, and being
 * a launch behind costs nothing.
 *
 * ponytail: no versioned filenames or update prompt. If waiting a launch ever
 * matters, bump CACHE and post a message to the page telling it to reload.
 */
self.addEventListener('fetch', (e) => {
  if (e.request.method !== 'GET') return;

  e.respondWith(
    caches.match(e.request).then((cached) => {
      const fresh = fetch(e.request)
        .then((response) => {
          if (response.ok) {
            const copy = response.clone();
            caches.open(CACHE).then((cache) => cache.put(e.request, copy));
          }
          return response;
        })
        .catch(() => cached); // offline: the cache is the only answer there is

      return cached || fresh;
    })
  );
});
