module.exports.getUserPublicKey = async function (body) { return { address: undefined } }
module.exports.getUserData = async function (body) { 
    return {
        data: Deno.core.ops.op_get_user_data()
    };
}