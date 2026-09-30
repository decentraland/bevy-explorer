#!/usr/bin/env python3
"""Static realm + content server for the scene-kill browser tests (PR #1091).

Serves a pointer-based realm whose entities/active answers with one raw-SDK7 test
scene per parcel cluster — each a variant of game.js with a different wedge mode
stamped in (see game.js header). Point the web client at it:

    http://localhost:5173/?realm=http://localhost:8111&position=<parcel>

  GET  /about                      realm manifest (offline comms)
  POST /content/entities/active    {"pointers": [...]} -> matching scene entities
  GET  /content/contents/<hash>    content files (game.js variants)

Parcels: graceful 0,0 | asyncwedge 40,0 | hangop 80,0 | spin 120,0
         opspin 160,0 | forge 200,0

Content hashes are derived from the stamped file bytes, so editing game.js
auto-busts the engine's Cache API content cache. Run: python3 serve.py [port]
"""

import hashlib
import json
import os
import sys
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

PORT = int(sys.argv[1]) if len(sys.argv) > 1 else 8111
HERE = os.path.dirname(os.path.abspath(__file__))

MODES = {
    "graceful": "0,0",
    "asyncwedge": "40,0",
    "hangop": "80,0",
    "spin": "120,0",
    "opspin": "160,0",
    "forge": "200,0",
}

with open(os.path.join(HERE, "game.js")) as f:
    TEMPLATE = f.read()

# hash -> bytes served at /content/contents/<hash>
CONTENTS = {}
# entity objects returned by entities/active
ENTITIES = []

for mode, parcel in MODES.items():
    body = TEMPLATE.replace("__MODE__", mode).encode()
    game_hash = "bafkkillgame" + hashlib.md5(body).hexdigest() + mode
    CONTENTS[game_hash] = body
    entity_id = "bafkkillscene" + hashlib.md5(game_hash.encode()).hexdigest() + mode
    entity = {
        "id": entity_id,
        "pointers": [parcel],
        "content": [{"file": "game.js", "hash": game_hash}],
        "metadata": {
            "main": "game.js",
            "scene": {"base": parcel, "parcels": [parcel]},
            "display": {"title": "kill-" + mode},
            "runtimeVersion": "7",
        },
    }
    CONTENTS[entity_id] = json.dumps(entity).encode()
    ENTITIES.append(entity)

ABOUT = {
    "content": {"healthy": True, "publicUrl": f"http://localhost:{PORT}/content"},
    "lambdas": {"healthy": True, "publicUrl": f"http://localhost:{PORT}/lambdas"},
    "comms": {"healthy": True, "protocol": "v3", "fixedAdapter": "offline:offline"},
    "configurations": {
        "realmName": "scene-kill-harness",
        "map": {
            "minimapEnabled": False,
            "sizes": [{"left": -5, "right": 210, "top": 5, "bottom": -5}],
        },
    },
}


class Handler(BaseHTTPRequestHandler):
    protocol_version = "HTTP/1.1"

    def _send(self, code, body=b"", ctype="application/json"):
        self.send_response(code)
        # the page is cross-origin-isolated (COEP credentialless); plain ACAO is enough
        # for its fetch()es, CORP covers anything loaded as a subresource
        self.send_header("Access-Control-Allow-Origin", "*")
        self.send_header("Access-Control-Allow-Methods", "GET, POST, OPTIONS")
        self.send_header("Access-Control-Allow-Headers", "*")
        self.send_header("Cross-Origin-Resource-Policy", "cross-origin")
        self.send_header("Content-Type", ctype)
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def do_OPTIONS(self):
        self._send(204)

    def do_GET(self):
        path = self.path.split("?")[0]
        if path == "/about":
            self._send(200, json.dumps(ABOUT).encode())
        elif path.startswith("/content/contents/"):
            h = path.rsplit("/", 1)[1]
            if h in CONTENTS:
                ctype = "application/javascript" if CONTENTS[h].startswith(b"//") else "application/json"
                self._send(200, CONTENTS[h], ctype)
            else:
                self._send(404, b"{}")
        else:
            self._send(404, b"{}")

    def do_POST(self):
        path = self.path.split("?")[0]
        length = int(self.headers.get("Content-Length") or 0)
        body = self.rfile.read(length) if length else b"{}"
        if path == "/content/entities/active":
            try:
                pointers = set(json.loads(body).get("pointers", []))
            except json.JSONDecodeError:
                pointers = set()
            matches = [e for e in ENTITIES if pointers & set(e["pointers"])]
            self._send(200, json.dumps(matches).encode())
        else:
            self._send(404, b"[]")

    def log_message(self, fmt, *args):
        sys.stderr.write("[serve] %s\n" % (fmt % args))


if __name__ == "__main__":
    for e in ENTITIES:
        print(f"[serve] {e['metadata']['display']['title']} @ {e['pointers'][0]} -> {e['id']}")
    print(f"[serve] realm about at http://localhost:{PORT}/about")
    ThreadingHTTPServer(("127.0.0.1", PORT), Handler).serve_forever()
