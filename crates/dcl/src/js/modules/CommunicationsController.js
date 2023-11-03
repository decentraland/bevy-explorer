module.exports.send = async function (body) { 
    console.log("send:", body.message);
    return await Deno.core.ops.op_comms_send(body.message);
}
