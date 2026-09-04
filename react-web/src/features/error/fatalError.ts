// The session's error state, shared by the two very different surfaces that render it:
//
// - `launch` / `runtime` / `react` are CRASHES — something broke. They go to <CrashModal/>, a
//   self-contained full-screen surface above everything that freezes HUD input (see inputLock), so
//   the screen stays exactly as it was for a screenshot/bug report.
// - `realm` is NOT a crash — the world the user asked for doesn't exist. It's an ordinary dialog on
//   the popup layer (see openRealmError): Escape closes it, nothing else is frozen.

export interface FatalError {
  message: string
  /**
   * 'launch' = boot panic (fatal), 'runtime' = engine crash (dismissable), 'react' = UI render
   * crash (fatal), 'realm' = the requested ?realm/world doesn't exist (dismissable → picker).
   */
  source: 'launch' | 'runtime' | 'react' | 'realm'
}

/** The crash sources — everything <CrashModal/> handles. `realm` is deliberately not one of them. */
export type CrashSource = Exclude<FatalError['source'], 'realm'>
