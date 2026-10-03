/** Unlike a mask, clipping removes the hole from pointer hit-testing too. */
export function holeClip({ x, y, width, height }: { x: number; y: number; width: number; height: number }): React.CSSProperties {
  return {
    clipPath: `polygon(evenodd, 0 0, 100% 0, 100% 100%, 0 100%, 0 0, ${x}px ${y}px, ${x + width}px ${y}px, ${x + width}px ${y + height}px, ${x}px ${y + height}px, ${x}px ${y}px, 0 0)`
  }
}
