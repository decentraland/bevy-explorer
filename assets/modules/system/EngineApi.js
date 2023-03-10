// engine module
module.exports.crdtSendToRenderer = async function(messages) {
    Deno.core.ops.op_crdt_send_to_renderer(messages.data)
}

module.exports.sendBatch = async function() {
    return { events: [] }
}

module.exports.crdtGetState = async function() {
    return { data: [] }
}

