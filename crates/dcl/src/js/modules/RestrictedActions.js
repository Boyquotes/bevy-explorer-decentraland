module.exports.movePlayerTo = async function (body) { 
    if ("cameraTarget" in body) {
        Deno.core.ops.op_move_player_to(false, [body.newRelativePosition.x, body.newRelativePosition.y, body.newRelativePosition.z], [target.x, target.y, target.z]);
    } else {
        Deno.core.ops.op_move_player_to(false, [body.newRelativePosition.x, body.newRelativePosition.y, body.newRelativePosition.z]);
    }
    return {} 
}

module.exports.teleportTo = async function (body) { 
    Deno.core.ops.op_move_player_to(true, [body.newRelativePosition.x, body.newRelativePosition.y, body.newRelativePosition.z])
    return {} 
}

module.exports.triggerEmote = async function (body) { return {} }
module.exports.changeRealm = async function (body) { return {} }
module.exports.openExternalUrl = async function (body) { return {} }
module.exports.openNftDialog = async function (body) { return {} }
module.exports.setCommunicationsAdapter = async function (body) { return {} }