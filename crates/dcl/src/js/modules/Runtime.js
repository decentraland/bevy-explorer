module.exports.getRealm = async function (body) { 
    return {
        realmInfo: await Deno.core.ops.op_realm_information()
    };
}

module.exports.getWorldTime = async function (body) { 
    // TODO
    return {} 
}

module.exports.readFile = async function (body) { 
    const res = await Deno.core.ops.op_read_file(body.fileName)
    return {
        content: new Uint8Array(res.content),
        hash: res.hash
    }
}

module.exports.getSceneInformation = async function (body) { 
    return await Deno.core.ops.op_scene_information();
}
