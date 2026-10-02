/** A CSS mask that leaves a rectangular hole (in the element's own CSS px) — for layers that sit
 *  over an engine-drawn view and must not cover it. */
export function holeMask(hole: { x: number; y: number; width: number; height: number }): React.CSSProperties {
  return {
    maskImage: 'linear-gradient(black, black), linear-gradient(black, black)',
    maskSize: `100% 100%, ${hole.width}px ${hole.height}px`,
    maskPosition: `0 0, ${hole.x}px ${hole.y}px`,
    maskRepeat: 'no-repeat',
    maskComposite: 'exclude',
    WebkitMaskComposite: 'xor'
  }
}
