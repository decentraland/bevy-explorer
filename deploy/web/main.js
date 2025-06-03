// Import the wasm-bindgen generated JS glue code and Rust functions
import init, {
    wasm_run,
} from './pkg/webgpu_build.js'; // Ensure this path is correct

const initialRealmInput = document.getElementById('initialRealm');
const locationInput = document.getElementById('location');
const initButton = document.getElementById('initButton');
const canvas = document.getElementById('mygame-canvas');

let initialRealmGroup = document.getElementById('initialRealm')?.parentElement;
let locationGroup = document.getElementById('location')?.parentElement;

function populateInputsFromQueryParams() {
    const queryParams = new URLSearchParams(window.location.search);
    const initialRealmParam = queryParams.get('initialRealm');
    if (initialRealmInput && initialRealmParam) {
        initialRealmInput.value = decodeURIComponent(initialRealmParam);
    } else if (initialRealmInput) {
        initialRealmInput.value = "https://realm-provider-ea.decentraland.org/main";
    }
    const locationParam = queryParams.get('location');
    if (locationInput && locationParam) {
        locationInput.value = decodeURIComponent(locationParam);
    } else if (locationInput) {
        locationInput.value = "0,0";
    }
}
function hideSettings() {
    if (initialRealmGroup) initialRealmGroup.style.display = 'none';
    if (locationGroup) locationGroup.style.display = 'none';
    if (initButton) initButton.style.display = 'none';
}

async function run() {
    populateInputsFromQueryParams();

    if (initButton) {
        initButton.disabled = true;
        initButton.textContent = 'Loading...';
    }

    const wasmUrl = './pkg/webgpu_build_bg.wasm';

    try {
        const compiledModule = await WebAssembly.compileStreaming(fetch(wasmUrl));

        const initialMemoryPages = 16384; 
        const maximumMemoryPages = 65536; 
        const sharedMemory = new WebAssembly.Memory({
            initial: initialMemoryPages,
            maximum: maximumMemoryPages, 
            shared: true
        });

        window.spawn_and_init_sandbox = async () => {
            return new Promise((resolve, _reject) => {
                const sandboxWorker = new Worker('sandbox_worker.js', { type: 'module' });
                sandboxWorker.onmessage = (workerEvent) => {
                    if (workerEvent.data.type === 'READY') {
                        sandboxWorker.postMessage({
                            type: 'INIT_WORKER',
                            payload: {
                                compiledModule,
                                sharedMemory
                            }
                        });
                    }
                    resolve()
                };
            });
        };

        await init({ module: compiledModule, memory: sharedMemory });
        console.log("[Main JS] Main application WebAssembly module initialized.");

        if (initButton) {
            initButton.disabled = false;
            initButton.textContent = 'Go';
        }

        initButton.onclick = () => {
            const initialRealm = initialRealmInput.value;
            const location = locationInput.value;
            console.log(`[Main JS] "Go" button clicked. Initial Realm: "${initialRealm}", Location: "${location}"`);
            hideSettings();
            wasm_run(initialRealm, location);
        };

    } catch (error) {
        console.error("[Main JS] Error during Wasm initialization or setup:", error);
        if (initButton) {
            initButton.textContent = 'Load Failed';
        }
    }
}

run().catch(console.error);
