// Simple development server with COOP/COEP headers for SharedArrayBuffer support
// Usage: node server.js [port] [--slow]
// --slow: Simulate slow network for testing loading UI

const http = require('http');
const fs = require('fs');
const path = require('path');

const args = process.argv.slice(2);
const PORT = args.find(a => !a.startsWith('--')) || 8080;
const SLOW_MODE = args.includes('--slow');

// Throttle settings (only apply when --slow is used)
const CHUNK_SIZE = 16 * 1024; // 16KB chunks
const CHUNK_DELAY_MS = 6;     // 6ms delay between chunks (~2.5 MB/s, 4G medio ~20 Mbps)

const MIME_TYPES = {
  '.html': 'text/html',
  '.js': 'text/javascript',
  '.css': 'text/css',
  '.json': 'application/json',
  '.png': 'image/png',
  '.jpg': 'image/jpeg',
  '.svg': 'image/svg+xml',
  '.wasm': 'application/wasm',
  '.ico': 'image/x-icon',
  '.webmanifest': 'application/manifest+json',
};

// Extensions to throttle in slow mode
const THROTTLE_EXTENSIONS = ['.wasm', '.js'];

function sleep(ms) {
  return new Promise(resolve => setTimeout(resolve, ms));
}

async function sendThrottled(res, data) {
  for (let i = 0; i < data.length; i += CHUNK_SIZE) {
    const chunk = data.slice(i, i + CHUNK_SIZE);
    res.write(chunk);
    await sleep(CHUNK_DELAY_MS);
  }
  res.end();
}

const server = http.createServer(async (req, res) => {
  // Add COOP/COEP headers for SharedArrayBuffer
  res.setHeader('Cross-Origin-Opener-Policy', 'same-origin');
  res.setHeader('Cross-Origin-Embedder-Policy', 'credentialless');

  let urlPath = req.url.split('?')[0];
  if (urlPath === '/') urlPath = '/index.html';
  let filePath = path.join(__dirname, urlPath);

  // Security: prevent directory traversal
  if (!filePath.startsWith(__dirname)) {
    res.writeHead(403);
    res.end('Forbidden');
    return;
  }

  const ext = path.extname(filePath).toLowerCase();
  const contentType = MIME_TYPES[ext] || 'application/octet-stream';

  fs.readFile(filePath, async (err, data) => {
    if (err) {
      if (err.code === 'ENOENT') {
        // Try index.html for SPA routing
        fs.readFile(path.join(__dirname, 'index.html'), (err2, data2) => {
          if (err2) {
            res.writeHead(404);
            res.end('Not Found');
          } else {
            res.writeHead(200, { 'Content-Type': 'text/html' });
            res.end(data2);
          }
        });
      } else {
        res.writeHead(500);
        res.end('Server Error');
      }
    } else {
      res.writeHead(200, {
        'Content-Type': contentType,
        'Content-Length': data.length
      });

      // Throttle certain file types in slow mode
      if (SLOW_MODE && THROTTLE_EXTENSIONS.includes(ext)) {
        console.log(`[SLOW] Throttling ${urlPath} (${(data.length / 1024).toFixed(0)} KB)`);
        await sendThrottled(res, data);
      } else {
        res.end(data);
      }
    }
  });
});

server.listen(PORT, () => {
  console.log(`Server running at http://localhost:${PORT}/`);
  console.log('COOP/COEP headers enabled for SharedArrayBuffer support');
  if (SLOW_MODE) {
    console.log(`SLOW MODE: Throttling .wasm and .js files (~${(CHUNK_SIZE / CHUNK_DELAY_MS).toFixed(0)} KB/s)`);
  }
});
