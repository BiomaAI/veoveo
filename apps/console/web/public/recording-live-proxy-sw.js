// Retirement endpoint for installed recording-proxy workers. New clients never
// register it. No fetch handler: requests use the browser's normal network path.
self.addEventListener("install", event => {
  event.waitUntil(self.skipWaiting());
});
self.addEventListener("activate", event => {
  event.waitUntil(self.registration.unregister());
});
