// What the HUD keeps while the scene editor is on the page: nothing of its chrome in 'edit',
// a player's minimum in 'play', everything when 'off'.
export type EditorHudMode = 'off' | 'edit' | 'play'

/** Whether the HUD still acts on an engine system action. `Cancel` always closes a popup; in
 *  'play' the emote wheel works; every page and panel hotkey stands down in both. */
export function hudActsOn(mode: EditorHudMode, action: string): boolean {
  if (mode === 'off' || action === 'Cancel') return true
  return mode === 'play' && (action === 'Emote' || action.startsWith('QuickEmote'))
}
