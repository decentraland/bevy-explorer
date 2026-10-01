// Loading-screen tips: which ones new and returning players see, and each tip's text, art and
// action pill.

import type { IconName } from '../../design'
import addFriends from '../../assets/loading-tips/add-friends.webp'
import earnBadges from '../../assets/loading-tips/earn-badges.webp'
import hangOutPlaza from '../../assets/loading-tips/hang-out-plaza.webp'
import liveNow from '../../assets/loading-tips/live-now.webp'
import makeMove from '../../assets/loading-tips/make-move.webp'
import sayHi from '../../assets/loading-tips/say-hi.webp'
import yourPeople from '../../assets/loading-tips/your-people.webp'
import addFriendIcon from '../../assets/loading/icon-add-friend.webp'
import favPlaceIcon from '../../assets/loading/icon-fav-place.webp'

/** Plain text, a highlighted word (a key: `binding` names the system action whose live key
 *  replaces it), or an inline icon. */
export type TipActionPart = string | { accent: string; binding?: string } | { icon: string }

export interface LoadingTip {
  key: string
  title: string
  body: string
  image: string
  action?: { icon?: IconName | { src: string }; iconSize?: number; parts: TipActionPart[] }
}

export const TIP_ROTATE_MS = 10_000
const RETURNING_AFTER_LAUNCHES = 3

// `take_shot` is left out: it points at an in-world camera we don't have.
const CATALOG: Record<string, LoadingTip> = {
  say_hi: {
    key: 'say_hi',
    title: 'Say Hi!',
    body: 'Open the chatbox and start a conversation\nwith people nearby.',
    image: sayHi,
    action: { icon: 'chat', parts: ['Press ', { accent: 'Enter', binding: 'Chat' }, ' to chat'] }
  },
  live_now: {
    key: 'live_now',
    title: 'Live Now',
    body: 'Find events, games, parties, and meetups happening right now.',
    image: liveNow,
    action: { icon: 'events', parts: ['Open ', { accent: 'Events' }, ' to see what’s on'] }
  },
  add_friends: {
    key: 'add_friends',
    title: 'Add Friends',
    body: 'Met someone cool? Add them so you can find each other again.',
    image: addFriends,
    action: { icon: { src: addFriendIcon }, parts: ['Open profile ', { accent: '→' }, ' Add friend'] }
  },
  make_move: {
    key: 'make_move',
    title: 'Make a Move',
    body: 'Emotes let you wave, react, or show off your moves without saying a word.',
    image: makeMove,
    action: { icon: 'emotes', parts: ['Press ', { accent: 'B', binding: 'Emote' }, ' to open Emotes Wheel'] }
  },
  hang_out: {
    key: 'hang_out',
    title: 'Hang Out',
    body: 'Genesis Plaza is where people hang out: by the fire, chatting, or crossing paths. Come by and see who’s around!',
    image: hangOutPlaza,
    action: { icon: 'map', iconSize: 50, parts: ['Open Map ', { accent: '→' }, { icon: favPlaceIcon }, 'Genesis Plaza'] }
  },
  your_people: {
    key: 'your_people',
    title: 'Your People',
    body: 'Find events, games, parties, and meetups happening right now.',
    image: yourPeople,
    action: { icon: 'communities', iconSize: 50, parts: ['Press ', { accent: 'O', binding: 'Communities' }, ' to search communities'] }
  },
  earn_badges: {
    key: 'earn_badges',
    title: 'Earn Badges',
    body: 'Badges show how you’ve spent your time: socializing, creating, or exploring. They appear on your profile for others to see.',
    image: earnBadges,
    action: { parts: ['Open profile ', { accent: '→' }, ' See Badges'] }
  }
}

type Audiences = { newUsers?: { displayed?: string[] }; returningUsers?: { displayed?: string[] } }

// Which tips new and returning players see (the reference's audience-loading-screen-tips rollout).
const AUDIENCES: Audiences = {
  newUsers: { displayed: ['say_hi', 'live_now', 'add_friends', 'make_move'] },
  returningUsers: { displayed: ['take_shot', 'hang_out', 'your_people', 'earn_badges', 'live_now'] }
}

/** The tips for new or returning players. */
export function tipsFor(launches: number): LoadingTip[] {
  const keys = (launches >= RETURNING_AFTER_LAUNCHES ? AUDIENCES.returningUsers : AUDIENCES.newUsers)?.displayed ?? []
  return keys.map((k) => CATALOG[k]).filter((t): t is LoadingTip => t != null)
}
