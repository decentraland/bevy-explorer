module.exports.getUserPublicKey = async function (body) { 
    const userData = await this.getUserData();

    return { address: userData.publicKey } 
}

module.exports.getUserData = async function (body) { 
    return {
        data: await Deno.core.ops.op_get_user_data()
    };
}