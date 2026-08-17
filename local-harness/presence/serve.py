#!/usr/bin/env python3
"""Minimal content + realm-about server for the presence-isolation harness.

Serves a static root directory by path (realm `about` docs and `/contents/<hash>`
entity + scene files that run.sh writes there), answers the client `entities/active`
pointer query with an empty list so synthetic clients load no scenes, and 404s the
rest. Bound to a fixed port passed as argv[1]; root dir as argv[2]."""

import http.server
import os
import sys

ROOT = os.path.abspath(sys.argv[2] if len(sys.argv) > 2 else ".")
PORT = int(sys.argv[1]) if len(sys.argv) > 1 else 8100


class Handler(http.server.BaseHTTPRequestHandler):
    def _send(self, code, body=b"", ctype="application/json"):
        self.send_response(code)
        self.send_header("Content-Type", ctype)
        self.send_header("Content-Length", str(len(body)))
        self.send_header("Access-Control-Allow-Origin", "*")
        self.end_headers()
        if body:
            self.wfile.write(body)

    def _serve_file(self, path):
        # strip leading slash, resolve within ROOT, block traversal
        rel = path.lstrip("/")
        full = os.path.abspath(os.path.join(ROOT, rel))
        if not full.startswith(ROOT) or not os.path.isfile(full):
            self._send(404, b'{"error":"not found"}')
            return
        with open(full, "rb") as f:
            self._send(200, f.read())

    def do_GET(self):
        path = self.path.split("?", 1)[0]
        self._serve_file(path)

    def do_POST(self):
        path = self.path.split("?", 1)[0]
        length = int(self.headers.get("Content-Length", 0))
        if length:
            self.rfile.read(length)
        if path.endswith("/entities/active"):
            # synthetic clients query this for scenes at their position; none exist
            self._send(200, b"[]")
        else:
            self._send(404, b'{"error":"not found"}')

    def log_message(self, *_):
        pass  # keep the harness output clean


if __name__ == "__main__":
    print(f"[serve] root={ROOT} port={PORT}", flush=True)
    http.server.ThreadingHTTPServer(("127.0.0.1", PORT), Handler).serve_forever()
