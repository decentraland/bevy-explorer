// A name for a new guest, offered before there is an account to derive one from: the engine's own
// word lists (crates/system_ui/src/login.rs GUEST_FIRST_NAMES / GUEST_LAST_NAMES), so a name
// picked here looks like one the engine would give. Every pair is a valid unclaimed name.
const FIRST = [
  'Astra',
  'Bolt',
  'Cinder',
  'Dusk',
  'Ember',
  'Flint',
  'Gale',
  'Haze',
  'Indigo',
  'Jade',
  'Kite',
  'Lumen',
  'Mica',
  'Nimbus',
  'Onyx',
  'Pixel',
  'Quill',
  'Rune',
  'Sable',
  'Talon',
  'Umber',
  'Vesper',
  'Wren',
  'Xenon',
  'Yarrow',
  'Zephyr',
  'Comet',
  'Delta',
  'Echo',
  'Nova'
]
const LAST = [
  'Ashgrove',
  'Deepwater',
  'Fernbrook',
  'Frostvale',
  'Glassmoor',
  'Greywood',
  'Highmoor',
  'Ironwood',
  'Mistgrove',
  'Moonvale',
  'Northwind',
  'Oakhollow',
  'Rainfield',
  'Riverbend',
  'Starfall',
  'Stonehill',
  'Sunridge',
  'Thornwood',
  'Wildmarsh',
  'Windward'
]

const pick = (words: string[]): string => words[Math.floor(Math.random() * words.length)]

// comms::profile::DEFAULT_LOOKS: the curated looks a new account gets, `default1` to `default160`
const DEFAULT_LOOKS = 160

/** One of the curated default looks, by number, never `previous`. */
export function randomDefaultLook(previous?: number): number {
  let look = previous
  while (look === previous) look = 1 + Math.floor(Math.random() * DEFAULT_LOOKS)
  return look as number
}

/** A random name, never `previous` (the dice always changes it). */
export function randomGuestName(previous?: string): string {
  let name = previous
  while (name === previous) name = pick(FIRST) + pick(LAST)
  return name as string
}
