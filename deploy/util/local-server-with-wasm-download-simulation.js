// Local dev server that simulates CDN behavior (Brotli compression on WASM files)
// This strips Content-Length just like Cloudflare does, so we can test manifest-based progress.
//
// Usage: node local-server.js [port]

const http = require("http");
const fs = require("fs");
const path = require("path");
const zlib = require("zlib");

const PORT = parseInt(process.argv[2] || "8080", 10);
const ROOT = __dirname;

const MIME_TYPES = {
  ".html": "text/html",
  ".js": "application/javascript",
  ".mjs": "application/javascript",
  ".css": "text/css",
  ".json": "application/json",
  ".wasm": "application/wasm",
  ".png": "image/png",
  ".svg": "image/svg+xml",
  ".ico": "image/x-icon",
  ".webmanifest": "application/manifest+json",
};

// File extensions that will be served with Brotli compression (simulating Cloudflare)
const COMPRESS_EXTENSIONS = new Set([".wasm"]);

const server = http.createServer((req, res) => {
  let urlPath = new URL(req.url, `http://localhost:${PORT}`).pathname;
  if (urlPath.endsWith("/")) urlPath += "index.html";

  const filePath = path.join(ROOT, urlPath);

  // Prevent directory traversal
  if (!filePath.startsWith(ROOT)) {
    res.writeHead(403);
    res.end("Forbidden");
    return;
  }

  fs.stat(filePath, (err, stats) => {
    if (err || !stats.isFile()) {
      res.writeHead(404);
      res.end("Not Found");
      return;
    }

    const ext = path.extname(filePath);
    const contentType = MIME_TYPES[ext] || "application/octet-stream";
    const acceptEncoding = req.headers["accept-encoding"] || "";
    const shouldCompress =
      COMPRESS_EXTENSIONS.has(ext) && acceptEncoding.includes("br");

    res.setHeader("Content-Type", contentType);
    res.setHeader("Cross-Origin-Opener-Policy", "same-origin");
    res.setHeader("Cross-Origin-Embedder-Policy", "require-corp");

    if (shouldCompress) {
      // Simulate Cloudflare: Brotli compress on the fly, NO Content-Length
      res.setHeader("Content-Encoding", "br");
      res.writeHead(200);
      fs.createReadStream(filePath).pipe(zlib.createBrotliCompress({
        params: { [zlib.constants.BROTLI_PARAM_QUALITY]: 0 }
      })).pipe(res);
    } else {
      res.setHeader("Content-Length", stats.size);
      res.writeHead(200);
      fs.createReadStream(filePath).pipe(res);
    }
  });
});

server.listen(PORT, () => {
  console.log(`Local server running at http://localhost:${PORT}`);
  console.log(`Serving files from ${ROOT}`);
  console.log(`WASM files served with Brotli compression (no Content-Length)`);
});
