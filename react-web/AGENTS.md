# react-web — Agent & Contributor Guidelines

This app is the React DOM port of the Decentraland in-world HUD. **The design system
is not optional — it is the only sanctioned way to build UI here.**

Design source of truth: the **Explorer 2.0 Figma**
(`Design System | Explorer 2.0`, file `CuOxttfA4jZ5I6gyH4YCsc`).

## 1. Always use the design system

- **Tokens are mandatory.** Every color, radius, spacing, shadow, z-index, font size,
  and motion value MUST come from `src/styles/tokens.css` (`var(--brand)`, `--panel`,
  `--text`, `--white-10`, `--green`, `--gold`, `--r-*`, `--fs-*`, `--dur-*`, …).
  **Never hardcode** a brand/status hex or rgba, a radius, or a type size in a
  component. The only raw values allowed are layout primitives with no token
  (e.g. a one-off `gap: 4px`) — and even those should prefer a token when one fits.

- **Primitives are mandatory.** Reusable interactive elements and surfaces MUST use the
  components in `src/design/` — `Button`, `IconButton`, `ControlButton`, `Panel`,
  `Icon`. Do **not** write a bespoke `<button>` + CSS for something a primitive already
  covers (close/back/menu/toggle buttons, nav controls, cards, popovers).

- **If a primitive doesn't exist, CREATE it** in `src/design/` — don't inline custom CSS
  for a pattern that will recur. A new primitive must:
  1. Live in `src/design/<Name>.tsx` (+ `.module.css`), token-driven only.
  2. Be exported from `src/design/index.ts`.
  3. Be added to `src/design/Showcase.tsx` (viewable at `?showcase=1`).
  4. Support variants via props (size/variant/tone), not copy-paste.

## 2. When custom CSS is acceptable

Feature-specific **layout/composition** (e.g. chat bubbles, member rows, the emoji
grid, the loading overlay) may use local CSS Modules — but they MUST still be
**100% token-driven** and should compose design-system primitives for every button,
input affordance, and surface they contain. Bespoke CSS is for *arrangement*, never for
re-implementing a primitive.

## 3. Before adding a component, ask:

1. Does a `src/design/` primitive already do this? → use it.
2. Is this a reusable control/surface with no primitive yet? → **create the primitive**,
   then use it.
3. Is this genuinely one-off feature layout? → CSS Module, token-driven, composing
   primitives.

## 4. Checks

- `npm run typecheck` must pass.
- No new hardcoded brand/status colors, radii, or type sizes — grep your diff for raw
  `#` hex and `rgba(` and justify each (most should be a token).

> Rationale: a single, enforced design system is what keeps the HUD consistent,
> themeable, and fast to evolve. Drifting into per-component custom CSS is the thing
> this file exists to prevent.
