// Settings: the explorer settings list + changing one.
//   from: BevyApi.getSettings() / setSetting().
//
// Also injects a synthetic "Graphics Preset" setting (Low/Medium/High/Custom) at the head
// of the list. Presets are defined here, scene-side — the engine only knows the individual
// settings, and its defaults match the Low preset. Setting the preset fans out to the
// individual setSetting calls; the preset's own value is derived by matching the live
// values against each preset (Custom when none match).
import { BevyApi } from '../bevy-api'
import type { Setting } from '../../../src/engine/protocol'
import type { Ctx } from '../bridge'

const PRESET_SETTING = 'Graphics Preset'
const PRESET_NAMES = ['Low', 'Medium', 'High']
const CUSTOM_INDEX = PRESET_NAMES.length

// Per-setting values for [Low, Medium, High]. Strings name an enum variant (resolved
// against the setting's namedVariants), numbers are raw slider values. Settings missing
// from the engine's list (platform-dependent) are skipped.
const PRESET_VALUES: Record<string, [number | string, number | string, number | string]> = {
  'Anti-aliasing': ['FXAA (Low)', 'FXAA (High)', 'FXAA (High)'],
  'Shadow Distance': [20, 100, 200],
  'Shadow settings': ['Low', 'High', 'High'],
  'Light Count': [4, 8, 32],
  'Shadow Caster Count': [0, 4, 8],
  Fog: ['Atmospheric', 'Atmospheric', 'Atmospheric'],
  Bloom: ['High', 'High', 'High'],
  'Depth of Field': ['High', 'High', 'High'],
  'Out-of-bounds Effect': ['On', 'On', 'On'],
  'Scene Load Distance': [10, 25, 100],
  'Scene Unload Distance': [10, 15, 20],
  'Distant Scene Rendering': ['Normal', 'Normal', 'Ultra'],
  'Empty Parcel Props': ['Low', 'Mid', 'High'],
  'Max Avatars': [20, 50, 100],
  'Max Videos': [1, 2, 4]
}

function resolveValue(setting: Setting, value: number | string): number | undefined {
  if (typeof value === 'number') return value
  const ix = setting.namedVariants.findIndex((v) => v.name === value)
  return ix < 0 ? undefined : ix
}

/** The (name, value) pairs preset `presetIx` would apply, given the live settings list. */
function presetTargets(settings: Setting[], presetIx: number): Array<{ name: string; value: number }> {
  const targets: Array<{ name: string; value: number }> = []
  for (const [name, values] of Object.entries(PRESET_VALUES)) {
    const setting = settings.find((s) => s.name === name)
    if (!setting) continue
    const value = resolveValue(setting, values[presetIx])
    if (value !== undefined) targets.push({ name, value })
  }
  return targets
}

/** Which preset the current values match exactly, or CUSTOM_INDEX if none. */
function currentPreset(settings: Setting[]): number {
  for (let ix = 0; ix < PRESET_NAMES.length; ix++) {
    const match = presetTargets(settings, ix).every(
      (t) => settings.find((s) => s.name === t.name)?.value === t.value
    )
    if (match) return ix
  }
  return CUSTOM_INDEX
}

function withPreset(settings: Setting[]): Setting[] {
  const preset: Setting = {
    name: PRESET_SETTING,
    category: 'Graphics',
    description:
      'Overall graphics quality. Picking a preset applies a bundle of the settings below; changing one of those settings individually shows here as Custom.',
    minValue: 0,
    maxValue: PRESET_NAMES.length + 1,
    namedVariants: [
      ...PRESET_NAMES.map((name) => ({ name, description: `Apply the ${name} preset.` })),
      { name: 'Custom', description: 'Individually adjusted settings.' }
    ],
    value: currentPreset(settings),
    default: 0,
    stepSize: 1
  }
  return [preset, ...settings]
}

export function registerSettings(ctx: Ctx): void {
  const pushSettings = (): void => {
    BevyApi.getSettings()
      .then((settings) => {
        ctx.send({ kind: 'settings', settings: withPreset(settings) })
      })
      .catch((e: unknown) => {
        console.error('[settings] getSettings failed', e)
      })
  }

  ctx.on('getSettings', () => {
    pushSettings()
  })
  ctx.on('setSetting', (msg) => {
    const apply =
      msg.name === PRESET_SETTING
        ? msg.value < PRESET_NAMES.length
          ? BevyApi.getSettings().then(async (settings) => {
              await Promise.all(
                presetTargets(settings, msg.value).map(async (t) => {
                  await BevyApi.setSetting(t.name, t.value)
                })
              )
            })
          : Promise.resolve() // Custom is a derived state, nothing to apply
        : BevyApi.setSetting(msg.name, msg.value)
    Promise.resolve(apply)
      .then(pushSettings)
      .catch((e: unknown) => {
        console.error('[settings] setSetting failed', e)
      })
  })

  pushSettings() // initial snapshot
}
