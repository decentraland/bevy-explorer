module.exports.send = async function (body) {
    await Deno.core.ops.op_comms_send_string(body.message);
    return {}
}

module.exports.sendBinary = async function (body) {
    // const data = (await Deno.core.ops.op_comms_send_binary([...body.data])).map((item) => new Uint8Array(item));
    for (const buffer of body.data) {
        await Deno.core.ops.op_comms_send_binary_single(new Uint8Array(buffer));
    }

    const data = (await Deno.core.ops.op_comms_recv_binary()).map((item) => new Uint8Array(item));
    return {
        data
    }
}
