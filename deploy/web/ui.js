// UI logic - NOT a module, executes immediately
// This file handles all UI-related functionality and can work without WASM

// Constants
const DEFAULT_SERVER = "https://realm-provider-ea.decentraland.org/main";
const DEFAULT_SYSTEMSCENE = "https://regenesislabs.github.io/bevy-ui-scene/BevyUiScene";

// DOM references
const initialRealmInput = document.getElementById("initialRealm");
const locationInput = document.getElementById("location");
const systemSceneInput = document.getElementById("systemScene");
const previewInput = document.getElementById("preview");
const initButton = document.getElementById("initButton");
const canvas = document.getElementById("canvas-parent");
const header = document.getElementById("header");

// Shared state
var autoStart = true;

/**
 * Populates input fields from URL query parameters.
 * Sets default values if no query params are provided.
 */
function populateInputsFromQueryParams() {
  const queryParams = new URLSearchParams(window.location.search);

  const manualParams = queryParams.get("manualParams");
  if (manualParams) {
    autoStart = false;
  }

  const initialRealmParam = queryParams.get("initialRealm");
  if (initialRealmInput && initialRealmParam) {
    initialRealmInput.value = decodeURIComponent(initialRealmParam);
  } else if (initialRealmInput) {
    initialRealmInput.value = DEFAULT_SERVER;
  }

  const locationParam = queryParams.get("location");
  if (locationInput && locationParam) {
    locationInput.value = decodeURIComponent(locationParam);
  } else if (locationInput) {
    locationInput.value = "";
  }

  const systemSceneParam = queryParams.get("systemScene");
  if (systemSceneInput && systemSceneParam) {
    systemSceneInput.value = decodeURIComponent(systemSceneParam);
  } else if (systemSceneInput) {
    systemSceneInput.value = DEFAULT_SYSTEMSCENE;
  }

  const previewParam = queryParams.get("preview");
  if (previewInput && previewParam) {
    previewInput.checked = true;
  } else if (previewInput) {
    previewInput.checked = false;
  }

  initialRealmInput.disabled = autoStart;
  locationInput.disabled = autoStart;
  systemSceneInput.disabled = autoStart;
  previewInput.disabled = autoStart;
}

/**
 * Hides the header and shows the canvas for the game.
 */
function hideHeader() {
  if (header) header.style.display = "none";
  if (canvas) canvas.style.display = "block";
}

/**
 * Updates the browser URL with the current game state.
 * Called from the WASM engine to keep URL in sync.
 */
window.set_url_params = (x, y, server, system_scene, preview) => {
  try {
    const urlParams = new URLSearchParams(window.location.search);

    urlParams.set("location", `${x},${y}`);

    if (server != DEFAULT_SERVER) {
      urlParams.set("initialServer", server);
    } else {
      urlParams.delete("initialServer");
    }

    if (system_scene != DEFAULT_SYSTEMSCENE) {
      urlParams.set("systemScene", system_scene);
    } else {
      urlParams.delete("systemScene");
    }

    if (preview) {
      urlParams.set("preview", true);
    } else {
      urlParams.delete("preview");
    }

    const newPath = window.location.pathname + '?' + urlParams.toString();
    history.replaceState(null, '', newPath);
  } catch (e) {
    console.log(`set url params failed: ${e}`);
  }
};

// Initialize UI immediately when script loads
populateInputsFromQueryParams();
