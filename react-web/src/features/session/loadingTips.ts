// Loading-screen tips. The remote flag `alfa-audience-loading-screen-tips` names which tips new and
// returning players see (by key); each key's text, art and action pill ship here. Without the
// flag, the older generic tips show instead.

import type { IconName } from '../../design'
import { flagPayload, type FeatureFlags } from '../../lib/featureFlags'
import addFriends from '../../assets/loading-tips/add-friends.webp'
import earnBadges from '../../assets/loading-tips/earn-badges.webp'
import hangOutPlaza from '../../assets/loading-tips/hang-out-plaza.webp'
import liveNow from '../../assets/loading-tips/live-now.webp'
import makeMove from '../../assets/loading-tips/make-move.webp'
import sayHi from '../../assets/loading-tips/say-hi.webp'
import yourPeople from '../../assets/loading-tips/your-people.webp'
import addFriendIcon from '../../assets/loading/icon-add-friend.webp'
import favPlaceIcon from '../../assets/loading/icon-fav-place.webp'
import badges from '../../assets/loading-tips/badges.webp'
import communities from '../../assets/loading-tips/communities.webp'
import creatorHub from '../../assets/loading-tips/creator-hub.webp'
import emotes from '../../assets/loading-tips/emotes.webp'
import events from '../../assets/loading-tips/events.webp'
import genesisCity from '../../assets/loading-tips/genesis-city.webp'
import hangOut from '../../assets/loading-tips/hang-out.webp'
import wearables from '../../assets/loading-tips/wearables.webp'
import worlds from '../../assets/loading-tips/worlds.webp'

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

// Key names the remote flag can list. `take_shot` is left out: it points at an in-world camera
// we don't have.
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

// The flag's live payload, used until the flags arrive so the first loading screen doesn't wait.
const DEFAULT_AUDIENCES = {
  newUsers: { displayed: ['say_hi', 'live_now', 'add_friends', 'make_move'] },
  returningUsers: { displayed: ['take_shot', 'hang_out', 'your_people', 'earn_badges', 'live_now'] }
}

type Audiences = { newUsers?: { displayed?: string[] }; returningUsers?: { displayed?: string[] } }

// `{Emote}` renders the live binding.
export const GENERIC_TIPS: LoadingTip[] = [
  { key: 'wearables', title: 'Wearables', body: 'Express yourself without limits! From accessories to full skins, the Marketplace has thousands of community-made Wearables for crafting your unique look.', image: wearables },
  { key: 'emotes', title: 'Emotes', body: "Wave to friends or show off your moves using {Emote} to trigger the Emote Wheel. Customize options from your Backpack so you're always ready to go!", image: emotes },
  { key: 'genesis_city', title: 'Genesis City', body: "Decentraland's open-world metropolis is made up of thousands of community-owned LAND parcels. Explore by foot or use the map to teleport—there's always something new to discover!", image: genesisCity },
  { key: 'events', title: 'Events', body: "From movie nights to dance parties, Decentraland's community-driven events are the best place to make friends! Browse the Event page and find what interests you.", image: events },
  { key: 'badges', title: 'Badges', body: 'Unlock badges by excelling at what you love—socializing, creating, or styling the perfect look in Decentraland—and show them off on your profile!', image: badges },
  { key: 'worlds', title: 'Worlds', body: 'Get a NAME, unlock a whole World! Separate from Genesis City, use your World to hang out, host events, or experiment with scene building.', image: worlds },
  { key: 'communities', title: 'Communities', body: 'Explore Communities to connect over shared interests. Hang in the group chat, get event updates, and enjoy that cozy sense of belonging!', image: communities },
  { key: 'creator_hub', title: 'Creator Hub', body: 'Build anything you can imagine from the perfect hangout, to alien worlds, or a full on gaming experience. Deploy to your World or LAND to invite the community!', image: creatorHub },
  { key: 'build_something', title: 'Build Something', body: "The Creator Hub gives you tools to build your own spaces, from simple hangouts to bigger experiences. What you build can become someone's regular spot.", image: creatorHub },
  { key: 'your_presence', title: 'Your Presence', body: "Badges reflect how you've spent time here: socializing, creating, or just being around. They show up on your profile so others get a sense of who they're meeting.", image: badges },
  { key: 'say_hi_emotes', title: 'Say Hi!', body: 'Emotes let you wave, react, or show off your moves without saying a word. Press {Emote} to open the Emote Wheel and join the moment.', image: emotes },
  { key: 'your_look', title: 'Your Look', body: 'Wearables shape how you appear over time. Made by the community, they become part of how people recognize you—and how you show off your style.', image: wearables },
  { key: 'your_people_generic', title: 'Your People', body: "Communities are how you find your people — from dance parties and chess matches to language practice, late-night talks, and art tours. Show up a few times and you start recognizing who's there.", image: communities },
  { key: 'whats_on', title: "What's On", body: "Movie nights, trivia, dance parties, there's usually something happening. Drop in enough times and you'll start to recognize the regulars.", image: events },
  { key: 'your_space', title: 'Your Space', body: "Your World is yours to do what you want with: build, experiment, hang out, host. You can also wander into other people's Worlds and see what they've put together.", image: worlds },
  { key: 'hang_out_generic', title: 'Hang Out', body: "Genesis Plaza is the place people tend to hang—around the fire pit, in conversation, crossing paths, feeding pigeons. Come by and see who's around!", image: hangOut }
]

/** The tips to show: the flag's list for new or returning players, or the generic set when the
 *  flags loaded without it. Before the flags load, the flag's last known lists. */
export function tipsFor(flags: FeatureFlags | null, launches: number): LoadingTip[] {
  const audiences = flags == null ? DEFAULT_AUDIENCES : (flagPayload(flags, 'alfa-audience-loading-screen-tips', 'tips') as Audiences | undefined)
  const keys = (launches >= RETURNING_AFTER_LAUNCHES ? audiences?.returningUsers : audiences?.newUsers)?.displayed ?? []
  const tips = keys.map((k) => CATALOG[k]).filter((t): t is LoadingTip => t != null)
  return tips.length > 0 ? tips : GENERIC_TIPS
}
