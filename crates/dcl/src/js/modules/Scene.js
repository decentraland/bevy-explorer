module.exports.getSceneInfo = async function (body) { 
    let scene_information = await Deno.core.ops.op_scene_information();
    
    return {
        cid: scene_information.urn,
        metadata: scene_information.metadataJson,
        ...scene_information
    } 
}
