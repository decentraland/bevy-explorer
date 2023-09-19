module.exports.getRealm = async function (body) { return {} }
module.exports.getWorldTime = async function (body) { return {} }
module.exports.readFile = async function (body) { 
    const res = await Deno.core.ops.op_read_file(body.fileName)
    return {
        content: new Uint8Array(res.content),
        hash: res.hash
    }
}

/*
export interface CurrentSceneEntityResponse {
    ** this is either the entityId or the full URN of the scene that is running *
    urn: string;
    ** contents of the deployed entities *
    content: ContentMapping[];
    ** JSON serialization of the entity.metadata field *
    metadataJson: string;
    ** baseUrl used to resolve all content files *
    baseUrl: string;
}
*/
module.exports.getSceneInformation = async function (body) { 
    return await Deno.core.ops.op_scene_information();
}
