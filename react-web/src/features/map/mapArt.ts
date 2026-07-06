// Map art — the actual Unity (unity-explorer) MapRenderer textures (AllIcn/ArtIcn/…,
// PinArt/PinGames/…), copied to src/assets/map under our category keys and statically
// imported (hashed, base-aware, a deleted/renamed file fails the BUILD).
import catAll from '../../assets/map/categories/all.png'
import catArt from '../../assets/map/categories/art.png'
import catBusiness from '../../assets/map/categories/business.png'
import catEducation from '../../assets/map/categories/education.png'
import catFashion from '../../assets/map/categories/fashion.png'
import catFavorites from '../../assets/map/categories/favorites.png'
import catGame from '../../assets/map/categories/game.png'
import catMusic from '../../assets/map/categories/music.png'
import catShop from '../../assets/map/categories/shop.png'
import catSocial from '../../assets/map/categories/social.png'
import catSports from '../../assets/map/categories/sports.png'
import pinArt from '../../assets/map/pins/art.png'
import pinBusiness from '../../assets/map/pins/business.png'
import pinEducation from '../../assets/map/pins/education.png'
import pinFashion from '../../assets/map/pins/fashion.png'
import pinFavorites from '../../assets/map/pins/favorites.png'
import pinGame from '../../assets/map/pins/game.png'
import pinMusic from '../../assets/map/pins/music.png'
import pinShop from '../../assets/map/pins/shop.png'
import pinSocial from '../../assets/map/pins/social.png'
import pinSports from '../../assets/map/pins/sports.png'
import worldPng from '../../assets/map/world.png'

export const CAT_ICONS: Record<string, string> = {
  all: catAll, art: catArt, business: catBusiness, education: catEducation, fashion: catFashion,
  favorites: catFavorites, game: catGame, music: catMusic, shop: catShop, social: catSocial,
  sports: catSports
}

export const CAT_PINS: Record<string, string> = {
  art: pinArt, business: pinBusiness, education: pinEducation, fashion: pinFashion,
  favorites: pinFavorites, game: pinGame, music: pinMusic, shop: pinShop, social: pinSocial,
  sports: pinSports
}

export const WORLD_ICON = worldPng
