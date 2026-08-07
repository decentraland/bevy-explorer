'use strict'

// Advisory postinstall check. The binary itself arrives through optionalDependencies;
// this only turns npm's silent optional-dependency skip (npm/cli#4828, and every
// --ignore-scripts / --no-optional install) into a readable message at install time
// rather than a confusing failure at first run.

const { resolveBinary, platformKey } = require('./lib/resolve-binary')

try {
  resolveBinary()
} catch (err) {
  const key = platformKey()
  process.stderr.write(
    `\n@dcl-regenesislabs/bevy-headless-server: ${err.message}\n` +
      (key
        ? `  Fix with: npm install @dcl-regenesislabs/bevy-headless-server-${key}@${require('./package.json').version}\n\n`
        : '\n')
  )
}
