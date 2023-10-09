module.exports.getPlayerData = async function (body) { 
    console.error("Player::getPlayerData not implemented");
    return {} 
}

module.exports.getPlayersInScene = async function (body) { 
    console.error("Player::getPlayersInScene not implemented");
    return {} 
}

module.exports.getConnectedPlayers = async function (body) { 
    let res = await Deno.core.ops.op_get_connected_players();
    return {
        players: res.map((address) => ({ userId: address }))
    } 
}
