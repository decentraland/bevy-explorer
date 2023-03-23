// engine module
module.exports.crdtSendToRenderer = async function(messages) {
    Deno.core.ops.op_crdt_send_to_renderer(messages.data);
    const data = (await Deno.core.ops.op_crdt_recv_from_renderer()).map((item) => new Uint8Array(item));
    return {
        data: data
    };
}

module.exports.sendBatch = async function() {
    return { events: [] }
}

module.exports.crdtGetState = async function() {
    const data = (await Deno.core.ops.op_crdt_recv_from_renderer()).map((item) => new Uint8Array(item))

    return {
        data: data
    };
}

