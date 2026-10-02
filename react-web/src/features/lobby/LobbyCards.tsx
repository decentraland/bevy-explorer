import jumpIn from '../../assets/lobby/jump-in.png'
import jumpInSolid from '../../assets/lobby/jump-in-solid.png'
import live from '../../assets/lobby/live.png'
import location from '../../assets/lobby/location.png'
import players from '../../assets/lobby/players.png'
import { Avatar, MaskIcon, VerifiedBadge } from '../../design'
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

function JumpInButton({ onClick, label = 'Jump in', disabled }: { onClick: () => void; label?: string; disabled?: boolean }): React.JSX.Element {
  return (
    <button
      type="button"
      className={styles.jumpIn}
      disabled={disabled}
      onClick={(e) => {
        e.stopPropagation()
        onClick()
      }}
    >
      <span>{label}</span>
      <MaskIcon src={jumpIn} size={24} />
    </button>
  )
}

export function LandingCard({
  title,
  creator,
  image,
  count,
  loading,
  onJumpIn
}: {
  title: string
  creator: string | null
  image: string | null
  count: number | null
  loading: boolean
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
        <JumpInButton onClick={onJumpIn} disabled={loading} />
      </div>
    </div>
  )
}

export function PlaceCard({
  title,
  creator,
  image,
  count,
  onJumpIn
}: {
  title: string
  creator: string | null
  image: string | null
  count: number
  onJumpIn: () => void
}): React.JSX.Element {
  return (
    <div className={styles.place}>
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
          <JumpInButton onClick={onJumpIn} />
        </div>
      </div>
    </div>
  )
}

export function LiveEventCard({ event, people, onJumpIn }: { event: DclEvent; people: number; onJumpIn: () => void }): React.JSX.Element {
  return (
    <div className={styles.liveEvent} role="button" tabIndex={0} onClick={onJumpIn} onKeyDown={(e) => e.key === 'Enter' && onJumpIn()}>
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
    </div>
  )
}

/** A place with people in it right now: the compact card, with its player count. */
export function LivePlaceCard({
  title,
  creator,
  image,
  count,
  onJumpIn
}: {
  title: string
  creator: string | null
  image: string | null
  count: number
  onJumpIn: () => void
}): React.JSX.Element {
  return (
    <div className={styles.upcoming} role="button" tabIndex={0} onClick={onJumpIn} onKeyDown={(e) => e.key === 'Enter' && onJumpIn()}>
      <div className={styles.upcomingText}>
        <div className={styles.upcomingName}>{title}</div>
        {creator && <div className={styles.upcomingHost}>{creator}</div>}
        <span className={styles.upcomingWhen}>
          <span className={styles.onlineDot} />
          <MaskIcon src={players} size={16} />
          {count} online
        </span>
      </div>
      <div className={styles.upcomingThumb}>{image && <img src={image} alt="" />}</div>
    </div>
  )
}

export function FriendCard({
  friend,
  color,
  where,
  onOpen,
  onJoin
}: {
  friend: Friend
  color: string
  where: string
  onOpen: () => void
  onJoin: (() => void) | null
}): React.JSX.Element {
  return (
    <div className={styles.friend} role="button" tabIndex={0} onClick={onOpen} onKeyDown={(e) => e.key === 'Enter' && onOpen()}>
      <Avatar src={friend.picture} name={friend.name} color={color} framed size={44} status={friend.status === 'away' ? 'away' : 'online'} />
      <div className={styles.friendName} style={{ color }}>
        <span>{friend.name}</span>
        {friend.claimed && <VerifiedBadge className={styles.verified} />}
      </div>
      <div className={styles.friendWhere}>
        <MaskIcon src={location} size={12} />
        <span>{where}</span>
      </div>
      {onJoin && (
        <button
          type="button"
          className={styles.friendJoin}
          onClick={(e) => {
            e.stopPropagation()
            onJoin()
          }}
        >
          Join
          <MaskIcon src={jumpInSolid} size={14} />
        </button>
      )}
    </div>
  )
}
