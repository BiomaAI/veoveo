const retiredScript = "/console/recording-live-proxy-sw.js";

/** Remove only the retired Console recording worker before session bootstrap. */
export async function retireRecordingWorker(): Promise<boolean> {
  if (!("serviceWorker" in navigator)) return false;
  const workers = navigator.serviceWorker;
  const retiredUrl = new URL(retiredScript, location.origin).href;
  const registration = await workers.getRegistration("/console/");
  const controlsPage = workers.controller?.scriptURL === retiredUrl;
  if (registration && [registration.active, registration.waiting, registration.installing]
    .some(worker => worker?.scriptURL === retiredUrl)) {
    await registration.unregister();
  }
  // Unregistering leaves this document controlled until its next navigation.
  if (controlsPage) {
    location.reload();
    return true;
  }
  return false;
}
