"use strict"
const engine = require("~engine");
const encode = (obj)=>JSON.stringify(obj);
const decode = (obj)=>JSON.parse(obj);
let incoming = [];
let outgoing = [];
async function sendReceive() {
    const newIncoming = (await engine.sendMessages(outgoing.map(encode))).map(decode);
    incoming = incoming.concat(newIncoming);
    outgoing = [];
}
const cubeId = 1;
let rotationX = 0;
let scaleY = 0;
let isSpaceBarPressed = 0;
module.exports.onStart = async function() {
    outgoing.push({
        method: "entity_add",
        data: {
            id: cubeId
        }
    });
    outgoing.push({
        method: "entity_transform_update",
        data: {
            entityId: cubeId,
            transform: {
                position: [
                    0,
                    0,
                    0
                ],
                rotation: [
                    0,
                    0,
                    0,
                    0
                ],
                scale: [
                    1,
                    1,
                    1
                ]
            }
        }
    });
    await sendReceive();
};
module.exports.onUpdate = async function(dt) {
    for (let msg of incoming){
        if (msg.method === "key_down" && msg.data.key === "space") {
            isSpaceBarPressed = true;
        }
        if (msg.method === "key_up" && msg.data.key === "space") {
            isSpaceBarPressed = false;
        }
    }
    incoming = [];
    if (isSpaceBarPressed) {
        scaleY += dt;
    } else {
        scaleY = Math.max(1.0, scaleY - dt);
    }
    rotationX += dt;
    outgoing.push({
        method: "entity_transform_update",
        data: {
            entityId: cubeId,
            transform: {
                position: [
                    0,
                    0,
                    0
                ],
                rotation: [
                    rotationX,
                    0,
                    0,
                    0
                ],
                scale: [
                    1,
                    scaleY,
                    1
                ]
            }
        }
    });
    await sendReceive();
};

