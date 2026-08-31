// The interrelated "how does this page boot" decisions, derived from the entry URL in ONE
// place. They cut across App.tsx (render gate), EngineHost (engine boot config) and
// useEngineSession (login/destination flow) — deriving them independently from raw params in
// each spot lets the combinations drift apart.
//
//   ?hud=0         render only the engine — no React HUD chrome (the sites /discover embed,
//                  which frames the scene itself; pair with ?guest=1 for a no-UI guest preview)
//   ?guest=1       skip the sign-in screen with an auto guest-login
//   ?systemScene=  debug: substitute the super-user ui scene (a url/dir, or "none" for the
//                  engine's builtin ui). The scene owns login/UI in-engine (pre-react
//                  behavior), so this implies a hidden HUD and no React login at all.
export interface BootMode {
  /** explicit ?systemScene= override; null = the default bridge scene drives */
  systemScene: string | null
  /** render only the engine — no HUD chrome, no sign-in/loading overlays */
  hideHud: boolean
  /** skip the sign-in screen: auto guest-login, or no React login at all (the ui scene owns it) */
  autoLogin: 'guest' | 'scene-owned' | null
}

// Computed per call, not a module const, so tests can set location.search before mounting.
export function bootMode(): BootMode {
  const q = new URLSearchParams(location.search)
  const systemScene = q.get('systemScene') || null
  return {
    systemScene,
    hideHud: q.get('hud') === '0' || systemScene != null,
    autoLogin: q.get('guest') === '1' ? 'guest' : systemScene != null ? 'scene-owned' : null
  }
}
