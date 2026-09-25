// Loading-screen tips: unity-explorer's default set (Locales/SceneLoadingDefaultTips + SceneLoadingDefaultImages),
// minus the in-world camera and Marketplace Credits tips (features we don't have). `{Emote}` renders the live binding.

import badges from '../../assets/loading-tips/badges.webp'
import communities from '../../assets/loading-tips/communities.webp'
import creatorHub from '../../assets/loading-tips/creator-hub.webp'
import emotes from '../../assets/loading-tips/emotes.webp'
import events from '../../assets/loading-tips/events.webp'
import genesisCity from '../../assets/loading-tips/genesis-city.webp'
import hangOut from '../../assets/loading-tips/hang-out.webp'
import wearables from '../../assets/loading-tips/wearables.webp'
import worlds from '../../assets/loading-tips/worlds.webp'

export interface LoadingTip {
  title: string
  body: string
  image: string
}

// Unity rotates every TipDisplayDuration (Plugin Settings.asset: 10s), in order.
export const TIP_ROTATE_MS = 10_000

export const LOADING_TIPS: LoadingTip[] = [
  { title: "Wearables", body: "Express yourself without limits! From accessories to full skins, the Marketplace has thousands of community-made Wearables for crafting your unique look.", image: wearables },
  { title: "Emotes", body: "Wave to friends or show off your moves using {Emote} to trigger the Emote Wheel. Customize options from your Backpack so you're always ready to go!", image: emotes },
  { title: "Genesis City", body: "Decentraland's open-world metropolis is made up of thousands of community-owned LAND parcels. Explore by foot or use the map to teleport—there's always something new to discover!", image: genesisCity },
  { title: "Events", body: "From movie nights to dance parties, Decentraland's community-driven events are the best place to make friends! Browse the Event page and find what interests you.", image: events },
  { title: "Badges", body: "Unlock badges by excelling at what you love—socializing, creating, or styling the perfect look in Decentraland—and show them off on your profile!", image: badges },
  { title: "Worlds", body: "Get a NAME, unlock a whole World! Separate from Genesis City, use your World to hang out, host events, or experiment with scene building.", image: worlds },
  { title: "Communities", body: "Explore Communities to connect over shared interests. Hang in the group chat, get event updates, and enjoy that cozy sense of belonging!\n", image: communities },
  { title: "Creator Hub", body: "Build anything you can imagine from the perfect hangout, to alien worlds, or a full on gaming experience. Deploy to your World or LAND to invite the community!", image: creatorHub },
  { title: "Build Something", body: "The Creator Hub gives you tools to build your own spaces, from simple hangouts to bigger experiences. What you build can become someone's regular spot.", image: creatorHub },
  { title: "Your Presence", body: "Badges reflect how you've spent time here: socializing, creating, or just being around. They show up on your profile so others get a sense of who they're meeting.", image: badges },
  { title: "Say Hi!", body: "Emotes let you wave, react, or show off your moves without saying a word. Press {Emote} to open the Emote Wheel and join the moment.", image: emotes },
  { title: "Your Look", body: "Wearables shape how you appear over time. Made by the community, they become part of how people recognize you—and how you show off your style.", image: wearables },
  { title: "Your People", body: "Communities are how you find your people — from dance parties and chess matches to language practice, late-night talks, and art tours. Show up a few times and you start recognizing who's there.", image: communities },
  { title: "What's On", body: "Movie nights, trivia, dance parties, there's usually something happening. Drop in enough times and you'll start to recognize the regulars.", image: events },
  { title: "Your Space", body: "Your World is yours to do what you want with: build, experiment, hang out, host. You can also wander into other people's Worlds and see what they've put together.", image: worlds },
  { title: "Hang Out", body: "Genesis Plaza is the place people tend to hang—around the fire pit, in conversation, crossing paths, feeding pigeons. Come by and see who's around!", image: hangOut }
]
