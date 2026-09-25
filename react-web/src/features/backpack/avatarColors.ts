// Which categories edit which avatar color, and Unity's presets for each
// (unity-explorer DCL/UI/ColorPicker/{BodyShape,Hair,Eyes}Colors.asset).

import type { AvatarColorTarget } from '../../engine/protocol'

export const COLOR_TARGET: Readonly<Record<string, AvatarColorTarget>> = {
  body_shape: 'skin',
  hair: 'hair',
  eyebrows: 'hair',
  facial_hair: 'hair',
  eyes: 'eyes'
}

export const COLOR_LABEL: Readonly<Record<AvatarColorTarget, string>> = { skin: 'Skin color', hair: 'Hair color', eyes: 'Eye color' }

export const COLOR_PRESETS: Readonly<Record<AvatarColorTarget, readonly string[]>> = {
  skin: ['#ffe4c6', '#ffddbc', '#f2c2a5', '#ddb18f', '#cc9b77', '#9a765b', '#7d5d47', '#704c38', '#522c1c', '#3c2216'],
  hair: ['#1c1c1c', '#3c210b', '#5b310f', '#7b4818', '#985f37', '#8c2014', '#e98234', '#ffbe28', '#fad281', '#d4d4d4'],
  eyes: ['#362626', '#5f3932', '#866142', '#bf9e5a', '#878078', '#afc5c7', '#20b3f6', '#397cb0', '#48dc75', '#3b9f50']
}
