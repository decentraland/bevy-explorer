/// <reference types="vite/client" />

// Build-time constant (vite.config.ts `define`): true in the native HUD bundle
// (`bundle:native`, --mode native) and on the dev server; FALSE in deployed web builds,
// where the minifier deletes every ?native=1 branch — a link must not be able to put a
// web deployment into native mode (it would skip the launch gates).
declare const __NATIVE_HUD__: boolean
