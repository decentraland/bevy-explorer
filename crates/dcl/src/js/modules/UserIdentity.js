module.exports.getUserPublicKey = async function (body) { 
    const userData = await Deno.core.ops.op_get_user_data();

    return { address: userData.publicKey } 
}

module.exports.getUserData = async function (body) { 
    return {
        data: await Deno.core.ops.op_get_user_data()
    };
}