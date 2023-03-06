// engine module
module.exports.sendMessages = async function(messages) {
    return await Deno.core.ops.op_engine_send_message(messages)
}
