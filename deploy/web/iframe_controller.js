// iframe_controller.js - Runs inside the sandboxed iframe

let sandboxWorker = null;

window.addEventListener('message', async (event) => {
    if (event.source !== window.parent) {
        console.warn("[Iframe Controller] Message received, but not from parent window. Origin:", event.origin, "Ignoring.");
        return;
    }

    if (event.data && event.data.type === 'INIT_SANDBOX') {
        if (sandboxWorker) {
            console.warn("[Iframe Controller] Sandbox worker already initialized. Ignoring new INIT_SANDBOX message.");
            return;
        }
        
        const payload = event.data.payload;
        if (!payload) {
             console.error("[Iframe Controller] 'INIT_SANDBOX' message missing payload.");
             return;
        }
        const { compiledModule, sharedMemory } = payload;

        if (!compiledModule || !(compiledModule instanceof WebAssembly.Module)) {
            console.error("[Iframe Controller] Invalid or missing WebAssembly.Module in payload. Received:", compiledModule);
            return;
        }
        if (!sharedMemory || !(sharedMemory instanceof WebAssembly.Memory)) {
            console.error("[Iframe Controller] Invalid or missing WebAssembly.Memory in payload. Received:", sharedMemory);
            return;
        }

        try {
            sandboxWorker = new Worker('sandbox_worker.js', { type: 'module' });
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
            };

            sandboxWorker.onerror = (error) => {
                console.error("[Iframe Controller] Error in Sandbox Web Worker:", error);
            };
        } catch (e) {
            console.error("[Iframe Controller] Error creating or messaging Sandbox Web Worker:", e);
        }
    } else if (event.data && event.data.type === 'TERMINATE_SANDBOX') {
        if (sandboxWorker) {
            sandboxWorker.terminate();
            sandboxWorker = null;
        }
    } else {
        console.log("[Iframe Controller] Received unhandled message or message with unexpected structure:", event.data);
    }
});
