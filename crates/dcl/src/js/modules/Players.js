module.exports.getPlayerData = async function (body) { 
    let res = await Deno.core.ops.op_get_player_data(body.userId);
    return {
        data: res
    };
}

module.exports.getPlayersInScene = async function (body) { 
    let res = await Deno.core.ops.op_get_players_in_scene();
    return {
        players: res.map((address) => ({ userId: address }))
    } 
}

module.exports.getConnectedPlayers = async function (body) { 
    let res = await Deno.core.ops.op_get_connected_players();
    return {
        players: res.map((address) => ({ userId: address }))
    } 
}
