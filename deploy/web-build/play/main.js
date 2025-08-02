// Import the wasm-bindgen generated JS glue code and Rust functions
import { initEngine, startEngine } from "../engine/engine.js";

const initialRealmInput = document.getElementById("initialRealm");
const locationInput = document.getElementById("location");
const systemSceneInput = document.getElementById("systemScene");
const initButton = document.getElementById("initButton");
const canvas = document.getElementById("mygame-canvas");

let initialRealmGroup = document.getElementById("initialRealm")?.parentElement;
let locationGroup = document.getElementById("location")?.parentElement;
let systemSceneGroup = document.getElementById("systemScene")?.parentElement;

function populateInputsFromQueryParams() {
  const queryParams = new URLSearchParams(window.location.search);
  const initialRealmParam = queryParams.get("initialRealm");
  if (initialRealmInput && initialRealmParam) {
    initialRealmInput.value = decodeURIComponent(initialRealmParam);
  } else if (initialRealmInput) {
    initialRealmInput.value = "https://realm-provider-ea.decentraland.org/main";
  }
  const locationParam = queryParams.get("location");
  if (locationInput && locationParam) {
    locationInput.value = decodeURIComponent(locationParam);
  } else if (locationInput) {
    locationInput.value = "0,0";
  }
  const systemSceneParam = queryParams.get("systemScene");
  if (systemSceneInput && systemSceneParam) {
    systemSceneInput.value = decodeURIComponent(systemSceneParam);
  } else if (systemSceneInput) {
    systemSceneInput.value = "";
  }
}
function hideSettings() {
  if (initialRealmGroup) initialRealmGroup.style.display = "none";
  if (locationGroup) locationGroup.style.display = "none";
  if (systemSceneGroup) systemSceneGroup.style.display = "none";
  if (initButton) initButton.style.display = "none";
}

async function run() {
  populateInputsFromQueryParams();

  if (initButton) {
    initButton.disabled = true;
    initButton.textContent = "Loading...";
  }

  try {
    await initEngine();

    if (initButton) {
      initButton.disabled = false;
      initButton.textContent = "Go";
    }
  } catch (e) {
    if (initButton) {
      console.log(e)
      initButton.textContent = "Load Failed";
    }
  }

  initButton.onclick = () => {
    const initialRealm = initialRealmInput.value;
    const location = locationInput.value;
    const systemScene = systemSceneInput.value;

    hideSettings();

    startEngine(initialRealm, location, systemScene);
  };
}

run().catch(console.error);
