// The scene's UI root.
//
// A scene gets exactly one renderer, and we have two engine-backed views that React shows
// through transparent cutouts — the Backpack's avatar preview and the minimap's Camera style.
// Both are composed here. Each returns null unless React has reported a rect for it, so this
// is empty in the common case.

import ReactEcs, { UiEntity } from '@dcl/react-ecs'
import { renderAvatarPreview } from './domains/avatarPreview'
import { renderMinimap } from './domains/minimap'

export function renderSceneUi(): ReactEcs.JSX.Element {
  return (
    <UiEntity uiTransform={{ positionType: 'absolute', position: { left: 0, top: 0 }, width: '100%', height: '100%' }}>
      {renderMinimap()}
      {/* Last, so it wins if both are ever up: the avatar preview paints an opaque
          full-screen backdrop. In practice they can't be — the minimap is HUD chrome and
          unmounts whenever a full-screen page, the Backpack included, is open. */}
      {renderAvatarPreview()}
    </UiEntity>
  )
}
