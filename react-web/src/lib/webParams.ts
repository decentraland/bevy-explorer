// The engine's entry-url parameter table, generated from crates/system_api_types/src/web_params.rs
// (scripts/gen-ts-bindings.sh → engine/generated/webParamTable.ts). Which params exist, how each
// reaches the engine and which ones a link may not set silently are all decided THERE; this
// module only derives the HUD's handling from it. Leaf module: imports nothing but the table.

import { WEB_PARAMS, type WebParam } from '../engine/generated'

/** A table row by name. Throws so a renamed/removed row fails at module load, not at launch. */
export function webParam(name: string): WebParam {
  const p = WEB_PARAMS.find((p) => p.name === name)
  if (p == null) throw new Error(`webParam: '${name}' is not in the engine's web param table`)
  return p
}

/** The `launch`-delivered params — what the host hands boot.js as window.__bevyBootConfig. */
export type LaunchOptions = Record<string, string | boolean | undefined>

/** Every `launch` param as it appears in the entry url; absent = undefined (the engine's default). */
export function launchOptionsFromUrl(q: URLSearchParams): LaunchOptions {
  const out: LaunchOptions = {}
  for (const p of WEB_PARAMS) {
    if (p.delivery !== 'launch') continue
    out[p.name] = p.kind === 'flag' ? q.has(p.name) : (q.get(p.name) ?? undefined)
  }
  return out
}
