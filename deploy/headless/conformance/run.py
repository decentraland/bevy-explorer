#!/usr/bin/env python3
"""Scene-conformance test for the headless engine.

Serves a minimal realm (about + active-entities + contents, mirroring the shape
`sdk-commands start` exposes and crates/ipfs consumes) for the prebuilt fixture
scene in ./scene, boots the engine binary against it, and asserts the five
[CONFORMANCE] lines the fixture prints — proving scene boot, isServer, raycasts
against colliders and the tween pipeline all work end to end with no GPU.

Usage:
    python3 run.py [path/to/headless]

The engine binary defaults to $CONFORMANCE_ENGINE_BIN, then target/debug/headless
(relative to the repo root). The dcl_deno_ipc sidecar must sit NEXT TO the engine
binary — the engine spawns it from its own directory.

Stdlib only; no pip installs. Exit code 0 iff every assertion passes and the
engine exits cleanly (it stops itself via --timeout).
"""

import base64
import hashlib
import http.server
import json
import os
import re
import socket
import subprocess
import sys
import threading
import time

HERE = os.path.dirname(os.path.abspath(__file__))
SCENE_DIR = os.path.join(HERE, "scene")
REPO_ROOT = os.path.abspath(os.path.join(HERE, "..", "..", ".."))

# wall-clock budget handed to the engine (--timeout); it exits 0 on its own when
# it lapses. The fixture completes in ~2s of scene time, so this is pure slack
# for slow CI runners.
ENGINE_TIMEOUT_SECS = 45
# grace beyond the engine's own timeout before we declare it hung and kill it
HARD_KILL_SECS = ENGINE_TIMEOUT_SECS + 60

# every assertion the fixture prints; matched against quote/ANSI-stripped output
ASSERTIONS = [
    "[CONFORMANCE] boot",
    "[CONFORMANCE] is-server true",
    "[CONFORMANCE] raycast-hit",  # coordinates are informational
    "[CONFORMANCE] tween-state",
    "[CONFORMANCE] tween-completed",
]

# files of the fixture entity, served under /content/contents/<hash>
CONTENT_FILES = {"scene.json": "scene.json", "bin/index.js": "bin/index.js"}

ANSI_RE = re.compile(r"\x1b\[[0-9;]*m")
WS_MAGIC = "258EAFA5-E914-47DA-95CA-C5AB0DC85B11"


def file_hash(relpath: str) -> str:
    # any stable string works as a hash for the engine; content-derived keeps
    # the entity id changing when the fixture is rebuilt (avoids stale caches)
    with open(os.path.join(SCENE_DIR, relpath), "rb") as f:
        return "b64-" + hashlib.sha256(f.read()).hexdigest()[:32]


class RealmHandler(http.server.BaseHTTPRequestHandler):
    protocol_version = "HTTP/1.1"
    server_version = "conformance-realm"

    # populated by serve():
    base_url = ""
    hashes = {}
    entity = {}

    def log_message(self, fmt, *args):
        print(f"[realm] {self.address_string()} {fmt % args}", flush=True)

    def _send_json(self, obj):
        body = json.dumps(obj).encode()
        self.send_response(200)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def _send_bytes(self, body: bytes, ctype="application/octet-stream"):
        self.send_response(200)
        self.send_header("Content-Type", ctype)
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def _not_found(self):
        self.send_response(404)
        self.send_header("Content-Length", "0")
        self.end_headers()

    def _path(self) -> str:
        # the engine builds some urls from publicUrl with joining slashes; be lenient
        return re.sub("/+", "/", self.path.split("?")[0])

    def do_GET(self):
        if "websocket" in self.headers.get("Upgrade", "").lower():
            return self._websocket_hold()
        path = self._path()
        if path == "/about":
            # minimal ServerAbout (crates/ipfs ServerAbout / sdk-commands about):
            # content.publicUrl drives entities/active + contents fetches; map bounds
            # make parcel 0,0 the world; offline comms keeps the engine solo.
            return self._send_json(
                {
                    "healthy": True,
                    "acceptingUsers": True,
                    "configurations": {
                        "realmName": "conformance",
                        "networkId": 1,
                        "map": {
                            "minimapEnabled": False,
                            "sizes": [{"left": 0, "right": 0, "top": 0, "bottom": 0}],
                        },
                    },
                    "content": {"healthy": True, "publicUrl": f"{self.base_url}/content"},
                    "lambdas": {"healthy": True, "publicUrl": f"{self.base_url}/lambdas"},
                    "comms": {"healthy": True, "protocol": "v3", "fixedAdapter": "offline:offline"},
                }
            )
        if path.startswith("/content/contents/") or path.startswith("/contents/"):
            requested = path.rsplit("/", 1)[1]
            if requested == self.entity["id"]:
                # the engine loads the entity definition itself by id from contents
                return self._send_json(self.entity)
            for rel, h in self.hashes.items():
                if h == requested:
                    with open(os.path.join(SCENE_DIR, rel), "rb") as f:
                        return self._send_bytes(f.read())
            return self._not_found()
        # /scenes (worlds index), lambdas, anything else: not provided
        return self._not_found()

    def do_POST(self):
        path = self._path()
        length = int(self.headers.get("Content-Length") or 0)
        body = self.rfile.read(length) if length else b"{}"
        if path.endswith("/entities/active"):
            try:
                pointers = json.loads(body).get("pointers", [])
            except json.JSONDecodeError:
                pointers = []
            print(f"[realm] entities/active for pointers={pointers}", flush=True)
            return self._send_json([self.entity])
        return self._not_found()

    def _websocket_hold(self):
        # the engine's preview hot-reload socket (crates/comms/src/preview.rs)
        # connects here; without a successful upgrade it warn-loops every frame.
        # Accept and hold open; we never send reload commands.
        key = self.headers.get("Sec-WebSocket-Key", "")
        accept = base64.b64encode(hashlib.sha1((key + WS_MAGIC).encode()).digest()).decode()
        self.send_response(101, "Switching Protocols")
        self.send_header("Upgrade", "websocket")
        self.send_header("Connection", "Upgrade")
        self.send_header("Sec-WebSocket-Accept", accept)
        self.end_headers()
        print("[realm] preview websocket connected (holding open)", flush=True)
        try:
            while self.rfile.read(1):
                pass  # discard client frames until the engine exits
        except OSError:
            pass
        self.close_connection = True


def build_entity(base_url: str):
    with open(os.path.join(SCENE_DIR, "scene.json")) as f:
        metadata = json.load(f)
    hashes = {rel: file_hash(rel) for rel in CONTENT_FILES}
    # distinct from every file hash — the engine fetches this id from /contents/
    # expecting the entity JSON itself; b64- prefix makes it always refresh its cache
    entity_id = "b64-" + hashlib.sha256(
        json.dumps(hashes, sort_keys=True).encode()
    ).hexdigest()[:32]
    entity = {
        "version": "v3",
        "id": entity_id,
        "type": "scene",
        "pointers": metadata["scene"]["parcels"],
        "timestamp": int(time.time() * 1000),
        "content": [{"file": rel, "hash": h} for rel, h in hashes.items()],
        "metadata": metadata,
    }
    return entity, hashes


def start_realm():
    server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), RealmHandler)
    port = server.server_address[1]
    base_url = f"http://127.0.0.1:{port}"
    RealmHandler.base_url = base_url
    RealmHandler.entity, RealmHandler.hashes = build_entity(base_url)
    threading.Thread(target=server.serve_forever, daemon=True).start()
    return server, base_url


def resolve_engine_bin() -> str:
    candidate = (
        (sys.argv[1] if len(sys.argv) > 1 else None)
        or os.environ.get("CONFORMANCE_ENGINE_BIN")
        or os.path.join(REPO_ROOT, "target", "debug", "headless")
    )
    candidate = os.path.abspath(candidate)
    if not os.path.isfile(candidate):
        sys.exit(f"engine binary not found: {candidate}")
    sidecar = os.path.join(os.path.dirname(candidate), "dcl_deno_ipc")
    if not os.path.isfile(sidecar) and not os.path.isfile(sidecar + ".exe"):
        sys.exit(f"dcl_deno_ipc sidecar not found next to the engine binary: {sidecar}")
    return candidate


def main() -> int:
    engine_bin = resolve_engine_bin()
    server, base_url = start_realm()
    print(f"[runner] realm on {base_url}, engine {engine_bin}", flush=True)

    env = dict(os.environ)
    env.setdefault("RUST_LOG", "warn,scene_runner=info")
    proc = subprocess.Popen(
        [
            engine_bin,
            "--realm",
            base_url,
            "--preview",
            "--server-mode",
            "--timeout",
            str(ENGINE_TIMEOUT_SECS),
        ],
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        env=env,
        cwd=os.path.dirname(engine_bin),
    )

    seen = set()
    killer = threading.Timer(HARD_KILL_SECS, proc.kill)
    killer.start()
    try:
        for raw in proc.stdout:
            line = raw.decode("utf-8", "replace").rstrip("\n")
            print(f"[engine] {line}", flush=True)
            # scene console.log args arrive quoted ("[CONFORMANCE] is-server" true)
            # and possibly colored; normalize before matching
            plain = ANSI_RE.sub("", line).replace('"', "")
            for assertion in ASSERTIONS:
                if assertion in plain:
                    seen.add(assertion)
        exit_code = proc.wait()
    finally:
        killer.cancel()
        proc.kill()
        server.shutdown()

    print(f"\n[runner] engine exited with code {exit_code}", flush=True)
    ok = True
    for assertion in ASSERTIONS:
        passed = assertion in seen
        ok &= passed
        print(f"[{'PASS' if passed else 'FAIL'}] {assertion}", flush=True)
    clean = exit_code == 0
    ok &= clean
    print(f"[{'PASS' if clean else 'FAIL'}] engine exits cleanly", flush=True)
    print(f"\n[runner] conformance {'PASSED' if ok else 'FAILED'}", flush=True)
    return 0 if ok else 1


if __name__ == "__main__":
    sys.exit(main())
