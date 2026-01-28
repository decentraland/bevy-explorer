// Main orchestrator - ES module
// Coordinates Service Worker registration, engine initialization, and startup

import { initEngine, start, gpu_cache_hash, initGpuCache } from "./engine.js";

// Service Worker registration
if ("serviceWorker" in navigator) {
  window.addEventListener("load", () => {
    const basePath = window.location.pathname.replace(/\/$/, ''); // removes trailing slash if present
    const serviceWorkerPath = new URL(`${basePath}/service_worker.js`, window.location.origin);

    navigator.serviceWorker
      .register(serviceWorkerPath)
      .then((registration) => {
        console.log(
          "Page: Service Worker registered successfully with scope: ",
          registration.scope
        );
      })
      .catch((error) => {
        console.log("Page: Service Worker registration failed: ", error);
      });
  });

  // make sure the worker stays around after a hard reload
  // 1. Check if a service worker is active and controlling the page.
  if (navigator.serviceWorker && navigator.serviceWorker.controller) {
    // SUCCESS CASE:
    // If the recovery flag is present, it means we just successfully
    // recovered from a hard reload. We can now remove the flag.
    if (sessionStorage.getItem('sw_reloaded')) {
      console.log('Service Worker recovery successful. Cleaning up flag.');
      sessionStorage.removeItem('sw_reloaded');
    }
    // Everything is fine, let the app load.
  } else {
    // 2. RECOVERY CASE: No service worker is in control.
    // This could be a first visit or a hard reload.
    if (navigator.serviceWorker && navigator.serviceWorker.getRegistration) {
      navigator.serviceWorker.getRegistration().then(registration => {
        // We only try to recover if a service worker is already registered.
        if (registration) {
          // Prevent an infinite reload loop.
          if (sessionStorage.getItem('sw_reloaded')) {
            sessionStorage.removeItem('sw_reloaded');
            console.error('Service Worker failed to take control after reload.');
          } else {
            // Set the flag and perform a standard reload.
            console.log('Page is uncontrolled. Reloading to activate Service Worker...');
            sessionStorage.setItem('sw_reloaded', 'true');
            window.location.reload();
          }
        }
      });
    }
  }
}

// Connect button click handler
initButton.onclick = start;

// Initialize engine and start or enable button
initEngine()
  .then(() => {
    // Step 5: GPU cache
    setLoadingStepActive('gpu');
    return initGpuCache(gpu_cache_hash());
  })
  .then(() => {
    setLoadingStepCompleted('gpu');
    hideLoading();

    if (autoStart) {
      start();
    } else {
      initButton.disabled = false;
      initButton.textContent = "Go";
    }
  })
  .catch((e) => {
    console.log("error", e);
    initButton.textContent = "Load Failed";
  });
