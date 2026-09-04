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
export type LaunchOptions = Record<string, string | boolean | number | undefined>

/** A url value in the type the engine expects for the param's kind. A number that doesn't parse
 *  is passed through as the string, so the engine's own error names it. */
function typedValue(p: WebParam, raw: string): string | boolean | number {
  switch (p.kind) {
    case 'bool':
      return raw === 'true' || raw === '1'
    case 'number': {
      const n = Number(raw)
      return raw.trim() !== '' && Number.isFinite(n) ? n : raw
    }
    default:
      return raw
  }
}

/** Every `launch` param as it appears in the entry url; absent = undefined (the engine's default). */
export function launchOptionsFromUrl(q: URLSearchParams): LaunchOptions {
  const out: LaunchOptions = {}
  for (const p of WEB_PARAMS) {
    if (p.delivery !== 'launch') continue
    if (p.kind === 'flag') {
      out[p.name] = q.has(p.name)
      continue
    }
    const raw = q.get(p.name)
    out[p.name] = raw == null ? undefined : typedValue(p, raw)
  }
  return out
}
