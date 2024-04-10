// engine module

const { op_crdt_recv_from_renderer, op_crdt_send_to_renderer, op_subscribe, op_send_batch } = Deno.core.ops;

module.exports.crdtSendToRenderer = async function(messages) {
    op_crdt_send_to_renderer(messages.data.buffer.slice(messages.data.byteOffset, messages.data.byteLength + messages.data.byteOffset));
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
    return {
        isServer: false
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
