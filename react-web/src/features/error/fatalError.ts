// The session's error state, shared by the two very different surfaces that render it:
//
// - `launch` / `runtime` / `react` are CRASHES — something broke. They go to <CrashModal/>, a
//   self-contained full-screen surface above everything that freezes HUD input (see inputLock), so
//   the screen stays exactly as it was for a screenshot/bug report.
// - `realm` and `bridge` are NOT crashes — the world the user asked for doesn't exist, or the HUD's
//   bridge scene never answered while the world itself runs fine. They are ordinary dialogs on the
//   popup layer (see openRealmError): Escape closes them, nothing else is frozen.

export interface FatalError {
  message: string
  /**
   * 'launch' = boot panic (fatal), 'runtime' = engine crash (dismissable), 'react' = UI render
   * crash (fatal), 'realm' = the requested ?realm/world doesn't exist (dismissable → picker),
   * 'bridge' = the HUD bridge scene never answered the handshake (dismissable, world keeps running).
   */
  source: 'launch' | 'runtime' | 'react' | 'realm' | 'bridge'
}

/** The crash sources — everything <CrashModal/> handles. `realm` and `bridge` are deliberately not
 *  among them. */
export type CrashSource = Exclude<FatalError['source'], DialogSource>
/** The not-a-crash sources, each shown as a titled dialog on the popup layer. */
export type DialogSource = 'realm' | 'bridge'
export const DIALOG_TITLE: Record<DialogSource, string> = {
  realm: 'World not found',
  bridge: 'HUD not connected'
}
export function isDialogSource(source: FatalError['source']): source is DialogSource {
  return source in DIALOG_TITLE
}
