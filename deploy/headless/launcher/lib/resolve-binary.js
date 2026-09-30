'use strict'

const fs = require('fs')
const path = require('path')

// Platform packages ship `headless` and `dcl_deno_ipc` side by side. The engine
// spawns the sidecar from its own directory under a fixed name, so the pair must
// never be split or copied apart.
const SUPPORTED = {
  'darwin arm64': 'darwin-arm64',
  'darwin x64': 'darwin-x64',
  'linux x64': 'linux-x64',
  'win32 x64': 'win32-x64'
}

function platformKey() {
  return SUPPORTED[`${process.platform} ${process.arch}`]
}

class UnsupportedPlatformError extends Error {}
class MissingBinaryError extends Error {}

function resolveFromPackage(key) {
  const pkg = `@dcl-regenesislabs/bevy-headless-server-${key}`
  const dir = path.dirname(require.resolve(`${pkg}/package.json`))
  return path.join(dir, 'bin')
}

/**
 * Absolute path to the `headless` executable.
 * DCL_BEVY_SERVER_PATH overrides everything (Creator Hub / pre-seeded installs).
 */
function resolveBinary() {
  const override = process.env.DCL_BEVY_SERVER_PATH
  if (override) {
    if (!fs.existsSync(override)) {
      throw new MissingBinaryError(`DCL_BEVY_SERVER_PATH points at a missing file: ${override}`)
    }
    return verifySidecar(override)
  }

  const key = platformKey()
  if (!key) {
    throw new UnsupportedPlatformError(
      `no bevy-headless build for ${process.platform}-${process.arch} ` +
        `(supported: ${Object.values(SUPPORTED).join(', ')})`
    )
  }

  let binDir
  try {
    binDir = resolveFromPackage(key)
  } catch (err) {
    throw new MissingBinaryError(
      `@dcl-regenesislabs/bevy-headless-server-${key} is not installed. ` +
        `If your package manager skipped optional dependencies, reinstall with them enabled ` +
        `or install that package explicitly.`
    )
  }

  const exe = path.join(binDir, process.platform === 'win32' ? 'headless.exe' : 'headless')
  if (!fs.existsSync(exe)) {
    throw new MissingBinaryError(`corrupt install: ${exe} is missing — reinstall the package`)
  }
  return verifySidecar(exe)
}

function verifySidecar(exe) {
  const sidecar = path.join(
    path.dirname(exe),
    process.platform === 'win32' ? 'dcl_deno_ipc.exe' : 'dcl_deno_ipc'
  )
  if (!fs.existsSync(sidecar)) {
    throw new MissingBinaryError(
      `the scene runtime sidecar is missing next to the engine (expected ${sidecar}). ` +
        `The two binaries must live in the same directory.`
    )
  }
  return exe
}

module.exports = { resolveBinary, platformKey, SUPPORTED, UnsupportedPlatformError, MissingBinaryError }
