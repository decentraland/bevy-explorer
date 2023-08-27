module.exports.getRealm = async function (body) { return {} }
module.exports.getWorldTime = async function (body) { return {} }
module.exports.readFile = async function (body) { 
    return await Deno.core.ops.op_read_file(body.fileName)
}
module.exports.getSceneInformation = async function (body) { return {} }