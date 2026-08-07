#!/usr/bin/env node
'use strict'

// Assembles one per-platform binary package from an already-built engine + sidecar.
//
//   node deploy/headless/build-platform-package.js \
//     --platform darwin-arm64 --version 0.1.0 \
//     --engine target/release/headless --sidecar target/release/dcl_deno_ipc \
//     --out deploy/headless/dist

const fs = require('fs')
const path = require('path')

const OS_CPU = {
  'darwin-arm64': { os: 'darwin', cpu: 'arm64' },
  'darwin-x64': { os: 'darwin', cpu: 'x64' },
  'linux-x64': { os: 'linux', cpu: 'x64' },
  'win32-x64': { os: 'win32', cpu: 'x64' }
}

function arg(name, fallback) {
  const i = process.argv.indexOf(`--${name}`)
  if (i === -1) {
    if (fallback !== undefined) return fallback
    throw new Error(`missing --${name}`)
  }
  return process.argv[i + 1]
}

const platform = arg('platform')
const version = arg('version')
const engine = arg('engine')
const sidecar = arg('sidecar')
const outRoot = arg('out', path.join(__dirname, 'dist'))

const target = OS_CPU[platform]
if (!target) throw new Error(`unknown platform ${platform}`)

const pkgDir = path.join(outRoot, `bevy-headless-server-${platform}`)
const binDir = path.join(pkgDir, 'bin')
fs.rmSync(pkgDir, { recursive: true, force: true })
fs.mkdirSync(binDir, { recursive: true })

const exeSuffix = target.os === 'win32' ? '.exe' : ''
for (const [src, name] of [
  [engine, `headless${exeSuffix}`],
  [sidecar, `dcl_deno_ipc${exeSuffix}`]
]) {
  fs.copyFileSync(src, path.join(binDir, name))
  fs.chmodSync(path.join(binDir, name), 0o755)
}

fs.writeFileSync(
  path.join(pkgDir, 'package.json'),
  JSON.stringify(
    {
      name: `@dcl-regenesislabs/bevy-headless-server-${platform}`,
      version,
      description: `bevy-headless engine + scene runtime sidecar for ${platform}`,
      license: 'Apache-2.0',
      repository: {
        type: 'git',
        url: 'https://github.com/decentraland/bevy-explorer.git'
      },
      os: [target.os],
      cpu: [target.cpu],
      // keep yarn PnP from zipping the package — the engine execs the sidecar off disk
      preferUnplugged: true,
      files: ['bin/']
    },
    null,
    2
  ) + '\n'
)

fs.writeFileSync(
  path.join(pkgDir, 'README.md'),
  `# @dcl-regenesislabs/bevy-headless-server-${platform}\n\n` +
    `Platform binaries for [@dcl-regenesislabs/bevy-headless-server](https://www.npmjs.com/package/@dcl-regenesislabs/bevy-headless-server).\n` +
    `Install that package instead; this one is selected automatically.\n\n` +
    `\`headless\` and \`dcl_deno_ipc\` must stay in the same directory — the engine execs the\n` +
    `sidecar from its own location.\n`
)

const sizes = fs
  .readdirSync(binDir)
  .map((f) => `${f} ${(fs.statSync(path.join(binDir, f)).size / 1e6).toFixed(1)} MB`)
  .join(', ')
process.stdout.write(`built ${pkgDir} (${sizes})\n`)
