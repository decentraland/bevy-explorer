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

  // Show form only when manualParams is present (autoStart is false)
  if (!autoStart) {
    const form = document.querySelector('form');
    if (form) form.style.display = 'block';
  }
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

// ============================================
// Loading Progress UI
// ============================================

const LOADING_STEPS = ['download', 'compile', 'init', 'workers', 'gpu'];
const loadingOverallFill = document.getElementById('loading-overall-fill');

// Track current step progress for overall bar calculation
let currentStepName = null;
let currentStepProgress = 0;

/**
 * Sets a loading step as active (shows spinner).
 * @param {string} stepName - The step identifier
 */
function setLoadingStepActive(stepName) {
  const step = document.querySelector(`.loading-step[data-step="${stepName}"]`);
  if (step) {
    step.classList.add('active');
    step.classList.remove('completed');
  }
  currentStepName = stepName;
  currentStepProgress = 0;
  updateOverallProgress();
}

/**
 * Sets a loading step as completed (shows checkmark).
 * @param {string} stepName - The step identifier
 */
function setLoadingStepCompleted(stepName) {
  const step = document.querySelector(`.loading-step[data-step="${stepName}"]`);
  if (step) {
    step.classList.remove('active');
    step.classList.add('completed');
  }
  currentStepName = null;
  currentStepProgress = 0;
  updateOverallProgress();
}

/**
 * Updates the progress bar of a specific step.
 * @param {string} stepName - The step identifier
 * @param {number} percent - Progress percentage (0-100)
 */
function setLoadingStepProgress(stepName, percent) {
  const step = document.querySelector(`.loading-step[data-step="${stepName}"]`);
  if (step) {
    const fill = step.querySelector('.loading-step-fill');
    if (fill) {
      fill.style.width = `${Math.min(100, Math.max(0, percent))}%`;
    }
  }
  // Update current step progress for overall bar
  if (stepName === currentStepName) {
    currentStepProgress = Math.min(100, Math.max(0, percent));
    updateOverallProgress();
  }
}

/**
 * Updates the overall progress bar based on completed steps + current step progress.
 * Each step is worth 1/5 (20%) of the total progress.
 */
function updateOverallProgress() {
  const completedCount = document.querySelectorAll('.loading-step.completed').length;
  const totalSteps = LOADING_STEPS.length;
  const stepWeight = 100 / totalSteps; // 20% per step

  // Completed steps + fraction of current step
  const percent = (completedCount * stepWeight) + (currentStepProgress / 100 * stepWeight);

  if (loadingOverallFill) {
    loadingOverallFill.style.width = `${percent}%`;
  }
}

/**
 * Hides the loading container.
 */
function hideLoading() {
  const container = document.getElementById('loading-container');
  if (container) container.style.display = 'none';
}

// Initialize UI immediately when script loads
populateInputsFromQueryParams();
