#!/usr/bin/env node
'use strict'

const { spawn } = require('child_process')
const path = require('path')
const {
  resolveBinary,
  UnsupportedPlatformError,
  MissingBinaryError
} = require('../lib/resolve-binary')

// Exit code 78 (EX_CONFIG) means "permanently unavailable here" — callers use it to
// fall back to another server implementation instead of retrying.
const EXIT_UNAVAILABLE = 78

const USAGE = `
  Decentraland authoritative scene server (bevy engine)

  Usage: bevy-headless-server --realm <url> [options]

    --realm <url>          Realm to serve. Required unless --orchestrated.
    --position <x,y>       Parcel to load. Default 0,0.
    --production           Production mode (disables preview-only behaviour).
    --orchestrated         Multi-scene mode driven over stdin by an orchestrator
                           (the multiplayer-server worker contract).
    --tick-hz <n>          Scene tick rate. Default 30.
    --timeout <secs>       Exit cleanly after N seconds.
    --version              Print version and exit.
    -h, --help             This message.

  Environment:
    DCL_BEVY_SERVER_PATH   Absolute path to a pre-installed \`headless\` binary,
                           bypassing the bundled platform package.
`

function fail(message, code) {
  process.stderr.write(`bevy-headless-server: ${message}\n`)
  process.exit(code)
}

/** Translate the hammurabi-server CLI contract into bevy-headless flags. */
function translate(argv) {
  const out = []
  let realm = null
  let position = null
  let production = false
  let orchestrated = false
  const passthrough = { '--tick-hz': true, '--timeout': true, '--scene-threads': true }

  for (let i = 0; i < argv.length; i++) {
    const arg = argv[i]
    // hammurabi accepts --flag=value; bevy's parser wants them separated.
    const eq = arg.indexOf('=')
    const name = eq === -1 ? arg : arg.slice(0, eq)
    const inlineValue = eq === -1 ? null : arg.slice(eq + 1)
    const takeValue = () => (inlineValue !== null ? inlineValue : argv[++i])

    switch (name) {
      case '--realm':
        realm = takeValue()
        break
      case '--position':
        position = takeValue()
        break
      case '--production':
        production = true
        break
      case '--orchestrated':
        orchestrated = true
        break
      case '-h':
      case '--help':
        process.stdout.write(USAGE)
        process.exit(0)
        break
      case '--version':
        process.stdout.write(`${require('../package.json').version}\n`)
        process.exit(0)
        break
      // Accepted by hammurabi, no equivalent here. Warn rather than fail so an
      // orchestrator passing extra flags still boots.
      case '--scene-id':
      case '--private-key':
      case '--env':
        takeValue()
        process.stderr.write(`bevy-headless-server: ignoring unsupported flag ${name}\n`)
        break
      default:
        if (passthrough[name]) {
          out.push(name, takeValue())
        } else {
          process.stderr.write(`bevy-headless-server: ignoring unknown flag ${arg}\n`)
        }
    }
  }

  // orchestrated mode gets scenes (with their content URLs) over stdin, so no realm
  if (!realm && !orchestrated) fail('--realm is required', EXIT_UNAVAILABLE)
  if (realm) {
    try {
      // eslint-disable-next-line no-new
      new URL(realm)
    } catch (e) {
      fail(`--realm is not a valid URL: ${realm}`, EXIT_UNAVAILABLE)
    }
  }
  if (position && !/^-?\d+,-?\d+$/.test(position)) {
    fail(`--position must be "x,y", got: ${position}`, EXIT_UNAVAILABLE)
  }
  // standalone (non-orchestrated) serving is a local-dev preview flow; the engine
  // refuses --server-mode without --preview
  if (production && !orchestrated) {
    fail('--production requires --orchestrated (standalone serving is preview-only)', EXIT_UNAVAILABLE)
  }

  const args = orchestrated ? ['--orchestrated'] : ['--server-mode']
  if (realm) args.unshift('--realm', realm)
  if (position) args.push('--location', position)
  if (!production) args.push('--preview')
  return args.concat(out)
}

function main() {
  const args = translate(process.argv.slice(2))

  let exe
  try {
    exe = resolveBinary()
  } catch (err) {
    if (err instanceof UnsupportedPlatformError || err instanceof MissingBinaryError) {
      fail(err.message, EXIT_UNAVAILABLE)
    }
    throw err
  }

  const child = spawn(exe, args, { stdio: 'inherit', env: process.env })

  child.on('error', (err) => {
    if (process.platform === 'linux' && /ENOENT|not found/i.test(err.message)) {
      process.stderr.write(
        'bevy-headless-server: the engine failed to start. On Linux it needs:\n' +
          '  sudo apt install libasound2 libudev1 libgl1 libx11-6 libxext6\n'
      )
      process.exit(EXIT_UNAVAILABLE)
    }
    fail(`failed to start the engine: ${err.message}`, EXIT_UNAVAILABLE)
  })

  const forward = (signal) => () => {
    if (!child.killed) child.kill(signal)
  }
  process.on('SIGTERM', forward('SIGTERM'))
  process.on('SIGINT', forward('SIGINT'))
  process.on('exit', forward('SIGTERM'))

  child.on('close', (code, signal) => {
    process.exit(signal ? 1 : code === null ? 1 : code)
  })
}

main()
