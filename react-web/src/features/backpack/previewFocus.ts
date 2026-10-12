// Which part of the avatar the preview camera frames for each category (head items zoom to the
// face, feet to the shoes; body-wide categories and "all" show the whole avatar).
import type { PreviewFocus } from '../../engine/protocol'

const HEAD = ['hair', 'eyebrows', 'eyes', 'mouth', 'facial_hair', 'hat', 'eyewear', 'earring', 'mask', 'tiara', 'top_head', 'helmet']

export function previewFocusFor(category: string): PreviewFocus {
  if (HEAD.includes(category)) return 'head'
  if (category === 'upper_body') return 'top'
  if (category === 'lower_body') return 'bottom'
  if (category === 'feet') return 'shoes'
  return 'body'
}
