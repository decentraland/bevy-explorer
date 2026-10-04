// Which entry-url params stop the boot on the UntrustedLaunchGate — THIS front-end's policy.
// The engine's web param table (lib/webParams.ts) says what a param does; whether a link may set
// it is decided here, because trust is a property of the host: the editor app trusts its editor
// scene, this app trusts its bundled bridge scene and Decentraland's own deployments.

import { SERVICES, type ServiceDef } from '../engine/generated'
import { BASE_DOMAIN, hostBaseDomain, isTrustedBaseDomain, normaliseServiceUrl } from './baseDomain'
import { bootMode } from './bootMode'
import { isTrustedSystemScene } from './systemScene'
import { webParam } from './webParams'

export interface UntrustedParam {
  name: string
  value: string
  /** what the param would let the link do — the gate's copy */
  warning: string
}

interface Gate {
  warning: string
  /** The effective value the engine will see, or null when the param is not in play. Read from
   *  the ENTRY url on first use — a later history.replaceState can't retire a warning. */
  read: (native: boolean) => string | null
  isTrusted: (value: string) => boolean
}

// Keyed by the table's param name (checked at module load).
const GATES: Record<string, Gate> = {
  // Substitutes the super-user scene — see lib/systemScene.ts for why that is a session takeover.
  systemScene: {
    warning:
      "Replaces the Explorer's interface with a scene loaded from this address. It can move your avatar, change your profile, and answer permission prompts on your behalf.",
    read: () => bootMode().systemScene,
    isTrusted: isTrustedSystemScene
  },
  // The HUD consumes this one itself (its own backend urls), so the value is the resolved
  // BASE_DOMAIN — normalised by the engine's rule, or derived from the origin when the param is
  // absent/invalid (both derive to a trusted deployment). Native is exempt: there the shell
  // injects it from the user's own --base-domain flag, not from a link.
  baseDomain: {
    warning:
      'Points every backend service — sign-in, content, comms — at servers under this domain. Whoever runs them would see your session and control what you play.',
    read: (native) => (native ? null : BASE_DOMAIN),
    isTrusted: isTrustedBaseDomain
  },
  // Repoints every scene and asset fetch at one server while everything else looks normal.
  contentServer: {
    warning:
      'Fetches every scene and asset from this server instead of the realm’s own. Whoever runs it decides what you see and what runs.',
    read: () => new URLSearchParams(location.search).get('contentServer'),
    isTrusted: isTrustedContentServer
  }
}

/** A content server under one of Decentraland's own deployments. */
function isTrustedContentServer(url: string): boolean {
  try {
    return hostBaseDomain(new URL(url).hostname) != null
  } catch {
    return false
  }
}
/** A service value — url, or `host[:port]` for an authority service — under one of Decentraland's own deployments. */
function isTrustedServiceUrl(s: ServiceDef, value: string): boolean {
  try {
    const u = new URL(s.value === 'authority' ? `dummy://${value}` : value)
    return hostBaseDomain(u.hostname) != null
  } catch {
    return false
  }
}
// Every per-service override: worse than ?baseDomain= in that it can poison ONE service —
// sign-in, profiles, comms — while everything else looks normal. Same rule as the content server,
// and native is exempt the same way as the base domain (the shell injects the user's own flags).
// Read normalised, like the base domain: a value the HUD discards is never put to the user.
for (const s of SERVICES) {
  GATES[s.name] = {
    warning: `Points the Explorer's "${s.name}" backend service at this server, with everything else looking normal. Whoever runs it would see the requests and choose the answers.`,
    read: (native) => (native ? null : normaliseServiceUrl(s, new URLSearchParams(location.search).get(s.name))),
    isTrusted: (value) => isTrustedServiceUrl(s, value)
  }
}
for (const name of Object.keys(GATES)) webParam(name)

/** The gated params whose entry-url value this front-end doesn't recognise. Empty = boot. */
export function untrustedLaunchParams({ native }: { native: boolean }): UntrustedParam[] {
  const out: UntrustedParam[] = []
  for (const [name, gate] of Object.entries(GATES)) {
    const value = gate.read(native)
    if (value == null || value === '' || gate.isTrusted(value)) continue
    out.push({ name, value, warning: gate.warning })
  }
  return out
}
