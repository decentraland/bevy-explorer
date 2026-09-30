// Scene-kill test scene (raw SDK7 ops, no @dcl/sdk toolchain). One instance per kill
// mode — serve.py stamps __MODE__ into this template and serves a variant per parcel,
// so the mode under test is picked by where you spawn (?position=). Scene-side facts
// go out as KILLTEST|<mode>|... console lines; the engine-side kill ladder logs
// ([Main JS] / [Sandbox Worker]) appear alongside them in the browser console.
//
// Modes:
//   graceful  - never wedges; kill via /reload (engine.reload() in the console) and
//               expect a tick-boundary exit, drained teardown, state freed, clean ack
//   asyncwedge- onUpdate parks on a bare promise (no op in flight): SHUTDOWN path,
//               drain sees refs==1, state freed
//   hangop    - onUpdate awaits getConnectedPlayers inline; the rpc is never flushed so
//               the op future parks forever holding scene state: SHUTDOWN path, drain
//               times out with refs>1, state + thread leaked (the leak branch)
//   spin      - pure js for(;;): SHUTDOWN ignored, forceful terminate after handshake
//   opspin    - for(;;) calling a sync webstorage op: op wrapper sees KILL and parks on
//               Atomics.wait; engine terminates the parked worker
//   forge     - posts untokened SHUTDOWN_COMPLETE/SCENE_READY and dispatches synthetic
//               SHUTDOWN + INIT_WORKER events; all must be dropped with warnings and the
//               scene must keep running (then kill via /reload like graceful)

const engine = require("~system/EngineApi");
const players = require("~system/Players");

const MODE = "__MODE__";
const WEDGE_AFTER_S = 8;

function log(kind, payload) {
  console.log("KILLTEST|" + MODE + "|" + kind + "|" + JSON.stringify(payload || {}));
}

let elapsed = 0;
let ticks = 0;
let wedged = false;

module.exports.onStart = async function () {
  log("scene-start");
};

module.exports.onUpdate = async function (dt) {
  ticks += 1;
  elapsed += dt;
  // keep the renderer fed while healthy (also drains the initial snapshot)
  await engine.crdtSendToRenderer({ data: new Uint8Array(0) });
  if (ticks % 60 === 0) log("alive", { tick: ticks, elapsed: Math.round(elapsed * 10) / 10 });

  if (MODE === "forge" && ticks === 30) {
    // every one of these must be dropped (with a warning) by the tokened message layer;
    // the scene must keep running, and still tear down cleanly on a later /reload
    log("forging");
    try {
      postMessage({ type: "SHUTDOWN_COMPLETE", sceneId: 12345 });
      postMessage({ type: "SCENE_READY", sceneId: 12345 });
    } catch (e) {
      log("forge-error", { which: "postMessage", msg: String(e) });
    }
    try {
      dispatchEvent(new MessageEvent("message", { data: { type: "SHUTDOWN" } }));
    } catch (e) {
      log("forge-error", { which: "shutdown", msg: String(e) });
    }
    try {
      dispatchEvent(new MessageEvent("message", {
        data: {
          type: "INIT_WORKER",
          payload: {
            compiledModule: {},
            sharedMemory: {},
            killToken: "forged",
            shutdownToken: "forged",
            killFlags: new SharedArrayBuffer(16),
          },
        },
      }));
    } catch (e) {
      log("forge-error", { which: "init", msg: String(e) });
    }
    log("forged", { note: "scene still running after forgeries" });
  }

  if (!wedged && elapsed >= WEDGE_AFTER_S) {
    wedged = true;
    if (MODE === "asyncwedge") {
      log("wedging", { how: "await-forever, no op in flight" });
      await new Promise(() => {});
    } else if (MODE === "hangop") {
      log("wedging", { how: "inline await getConnectedPlayers; rpc never flushed, op future holds scene state" });
      await players.getConnectedPlayers();
      log("unreachable", { note: "hangop resolved?!" });
    } else if (MODE === "spin") {
      log("wedging", { how: "pure js for(;;)" });
      for (;;) {}
    } else if (MODE === "opspin") {
      log("wedging", { how: "for(;;) calling a sync webstorage op" });
      for (;;) {
        localStorage.getItem("kill-test");
      }
    }
    // graceful / forge: no wedge — kill via /reload in the console
  }
};
