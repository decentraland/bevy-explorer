module.exports.getRealm = async function (body) { return {} }
module.exports.getWorldTime = async function (body) { return {} }
module.exports.readFile = async function (body) { 
    const res = await Deno.core.ops.op_read_file(body.fileName)
    return {
        content: new Uint8Array(res.content),
        hash: res.hash
    }
}
module.exports.getSceneInformation = async function (body) { return {} }
