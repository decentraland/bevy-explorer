// Nearby voice popover beside the sidebar voice button: hear others on/off, their volume, and a
// Speak toggle. Closes on an outside click or Cancel; turning hearing off closes it too.

import { useEffect, useLayoutEffect, useRef, useState } from 'react'
import { MaskIcon, Slider, Toggle } from '../../design'
import { registerCancelLayer } from '../../lib/cancelLayers'
import hearIcon from '../../assets/voice/hear.webp'
import speakerIcon from '../../assets/voice/speaker.webp'
import speakIcon from '../../assets/voice/speak.webp'
import speakingIcon from '../../assets/voice/speaking.webp'
import styles from './NearbyVoiceWidget.module.css'

export function NearbyVoiceWidget({
  anchor,
  hearing,
  onHearingChange,
  volume,
  onVolumeChange,
  speaking,
  talking,
  micAvailable,
  onSpeakToggle,
  talkKey,
  onClose
}: {
  /** The sidebar voice button the popover sits beside. */
  anchor: HTMLElement | null
  hearing: boolean
  onHearingChange: (on: boolean) => void
  /** 0–100. */
  volume: number
  onVolumeChange: (volume: number) => void
  /** The mic is open. */
  speaking: boolean
  /** Voice is being picked up right now. */
  talking: boolean
  micAvailable: boolean
  onSpeakToggle: () => void
  /** The push-to-talk key's label, when it has one. */
  talkKey?: string
  onClose: () => void
}): React.JSX.Element {
  const ref = useRef<HTMLDivElement>(null)
  const [top, setTop] = useState<number | null>(null)
  const [left, setLeft] = useState(0)

  useLayoutEffect(() => {
    if (anchor == null) return
    const r = anchor.getBoundingClientRect()
    setTop(r.top + r.height / 2)
    // 34 canvas px right of the button centre.
    setLeft(r.left + r.width / 2 + r.width)
  }, [anchor])

  useEffect(() => {
    const onDown = (e: MouseEvent): void => {
      const t = e.target as Node
      if (ref.current?.contains(t) || anchor?.contains(t)) return
      onClose()
    }
    document.addEventListener('mousedown', onDown)
    const off = registerCancelLayer(onClose)
    return () => {
      document.removeEventListener('mousedown', onDown)
      off()
    }
  }, [anchor, onClose])

  return (
    <div ref={ref} className={styles.panel} role="dialog" aria-label="Nearby voice" style={{ top: top ?? -9999, left }}>
      <div className={styles.title}>NEARBY VOICE</div>
      <div className={styles.divider} />
      <div className={styles.row}>
        <MaskIcon src={hearIcon} size={20} />
        <span className={styles.label}>Hear others</span>
        <Toggle size="sm" checked={hearing} onChange={onHearingChange} aria-label="Hear others" />
      </div>
      {hearing && (
        <>
          <div className={styles.row}>
            <MaskIcon src={speakerIcon} size={20} />
            <div className={styles.slider}>
              <Slider variant="thick" value={volume} onChange={onVolumeChange} aria-label="Voice volume" />
            </div>
          </div>
          <button
            type="button"
            className={`${styles.speak} ${speaking ? styles.speakOn : ''} ${speaking && talking ? styles.speakLive : ''}`.trim()}
            aria-pressed={speaking}
            disabled={!micAvailable}
            onClick={onSpeakToggle}
          >
            {speaking ? (
              <>
                <img src={speakingIcon} alt="" width={25} height={25} />
                <span className={styles.speakingText}>Speaking</span>
              </>
            ) : (
              <>
                <MaskIcon src={speakIcon} size={20} />
                Speak
              </>
            )}
          </button>
          <div className={styles.hint}>
            {!micAvailable ? (
              'No microphone available'
            ) : talkKey == null ? null : speaking ? (
              <>
                Press <span className={styles.key}>[{talkKey}]</span> to stop speaking
              </>
            ) : (
              <>
                Hold <span className={styles.key}>[{talkKey}]</span> to speak momentarily
              </>
            )}
          </div>
        </>
      )}
    </div>
  )
}
