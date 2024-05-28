module.exports.getTextureSize = async function (body) {
    const size = await Deno.core.ops.op_get_texture_size(body.src)
    return { src: body.src, size }
}
