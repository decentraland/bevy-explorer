// engine module

const { op_crdt_recv_from_renderer, op_crdt_send_to_renderer, op_subscribe, op_send_batch, op_is_server } = Deno.core.ops;

module.exports.crdtSendToRenderer = async function(messages) {
    op_crdt_send_to_renderer(messages.data);
    const data = (await op_crdt_recv_from_renderer()).map((item) => new Uint8Array(item));
    return {
        data: data
    };
}

module.exports.crdtGetState = async function() {
    const data = (await op_crdt_recv_from_renderer()).map((item) => new Uint8Array(item))

    return {
        data: data
    };
}

module.exports.isServer = async function() {
    // Feature-detected + synchronous: op_is_server returns a bool. Guard keeps the
    // web/wasm build safe if the op is absent (evaluates to false). NEVER await an
    // async variant here — a Promise is always truthy and would make clients act as servers.
    return {
        isServer: !!(op_is_server && op_is_server())
    }
}

/**
 * @deprecated this is an SDK6 API.
 * This function subscribe to an event from the renderer
 */
module.exports.subscribe = async function(message) {
    op_subscribe(message.eventId);
}

/**
 * @deprecated this is an SDK6 API.
 * This function unsubscribe to an event from the renderer
 */
module.exports.unsubscribe = async function(message) {
    op_subscribe(message.eventId);
}

/**
 * @deprecated this is an SDK6 API.
 * This function polls events from the renderer
 */
module.exports.sendBatch = async function() {
    return { events: op_send_batch() }
}
