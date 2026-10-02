import jumpIn from '../../assets/lobby/jump-in.png'
import live from '../../assets/lobby/live.png'
import location from '../../assets/lobby/location.png'
import players from '../../assets/lobby/players.png'
import { Avatar, Button, MaskIcon, VerifiedBadge } from '../../design'
import type { DclEvent } from '../events/eventsApi'
import type { Friend } from '../../engine/protocol'
import styles from './LobbyCards.module.css'

function OnlineCounter({ count }: { count: number }): React.JSX.Element {
  return (
    <span className={styles.counter}>
      <span className={styles.onlineDot} />
      <MaskIcon src={players} size={16} />
      {count}
    </span>
  )
}

function JumpIn({ size, disabled, onClick }: { size: 'card' | 'cardSm'; disabled?: boolean; onClick: () => void }): React.JSX.Element {
  return (
    <Button size={size} disabled={disabled} onClick={onClick}>
      Jump in
      <MaskIcon src={jumpIn} size={24} />
    </Button>
  )
}

export function LandingCard({
  title,
  creator,
  image,
  count,
  loading,
  disabled,
  onJumpIn
}: {
  title: string
  creator: string | null
  image: string | null
  count: number | null
  loading: boolean
  disabled: boolean
  onJumpIn: () => void
}): React.JSX.Element {
  return (
    <div className={styles.landing} aria-busy={loading}>
      <div className={`${styles.thumb} ${loading ? styles.skeleton : ''}`.trim()}>{image && <img src={image} alt="" />}</div>
      {count != null && !loading && (
        <div className={styles.badges}>
          <OnlineCounter count={count} />
        </div>
      )}
      <div className={styles.landingTitle}>{title}</div>
      {creator && <div className={styles.landingCreator}>{creator}</div>}
      <div className={styles.landingJump}>
        <JumpIn size="card" disabled={loading || disabled} onClick={onJumpIn} />
      </div>
    </div>
  )
}

export function PlaceCard({
  title,
  creator,
  image,
  count,
  wide = false,
  disabled,
  onJumpIn
}: {
  title: string
  creator: string | null
  image: string | null
  count: number
  /** Fill its column (the Live Now rail) instead of the rails' 280px. */
  wide?: boolean
  disabled: boolean
  onJumpIn: () => void
}): React.JSX.Element {
  return (
    <div className={`${styles.place} ${wide ? styles.placeWide : ''}`.trim()}>
      <div className={styles.placeHeader}>
        <div className={styles.thumb}>{image && <img src={image} alt="" />}</div>
        <div className={styles.badges}>
          <OnlineCounter count={count} />
        </div>
      </div>
      <div className={styles.placeFooter}>
        <div className={styles.placeTitle}>{title}</div>
        {creator && <div className={styles.placeCreator}>{creator}</div>}
        <div className={styles.placeJump}>
          <JumpIn size="cardSm" disabled={disabled} onClick={onJumpIn} />
        </div>
      </div>
    </div>
  )
}

export function LiveEventCard({
  event,
  people,
  onJumpIn
}: {
  event: DclEvent
  people: number
  /** null when the event has no place to go to. */
  onJumpIn: (() => void) | null
}): React.JSX.Element {
  return (
    <button type="button" className={styles.liveEvent} disabled={onJumpIn == null} onClick={onJumpIn ?? undefined}>
      <div className={styles.thumb}>{event.image && <img src={event.image} alt="" />}</div>
      <div className={styles.badges}>
        <span className={styles.liveBadge}>
          <MaskIcon src={live} size={16} />
          Live
        </span>
        <OnlineCounter count={people} />
      </div>
      <div className={styles.landingTitle}>{event.name}</div>
      {event.user_name && <div className={styles.landingCreator}>By {event.user_name}</div>}
    </button>
  )
}

export function FriendCard({ friend, color, where, onOpen }: { friend: Friend; color: string; where: string; onOpen: () => void }): React.JSX.Element {
  return (
    <button type="button" className={styles.friend} onClick={onOpen}>
      <Avatar src={friend.picture} name={friend.name} color={color} framed size={44} status={friend.status === 'away' ? 'away' : 'online'} />
      <span className={styles.friendName} style={{ color }}>
        <span>{friend.name}</span>
        {friend.claimed && <VerifiedBadge className={styles.verified} />}
      </span>
      <span className={styles.friendWhere}>
        <MaskIcon src={location} size={12} />
        <span>{where}</span>
      </span>
    </button>
  )
}
