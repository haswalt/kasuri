# Styling the UI

The React frontend uses Panda CSS v2.5 with the official Vite plugin. Panda generates static CSS at build time; the backend remains Rust/Wasm.

## Source layout

- `apps/web/panda.config.ts` registers tokens, conditions and recipes.
- `apps/web/theme/tokens.ts` owns the handoff palette, fonts, sizes, radii and shadows. Semantic colours and shadows support explicit light/dark themes and the device's preferred theme when no attribute is set.
- `apps/web/theme/recipes/primitives.ts` defines button visual/tone variants, status pills, category tiles, workflow node states/selection and connector error ports.
- Other files in `apps/web/theme/recipes/` define multi-part slot recipes for the shell, menus, dashboard, editor, palette, inspector, debugger, catalogue, settings and login.
- `apps/web/src/design/styles.ts` exposes the typed slot classes used by components. `apps/web/src/design/ui.tsx` exposes shared React primitives.
- `apps/web/theme/global.ts` contains native element defaults, accessibility/motion defaults, React Flow integration and legacy CSS-variable aliases used by runtime charts. Those aliases reference Panda tokens; they do not define another palette.
- `apps/web/src/index.css` imports local fonts, places React Flow in the base layer and declares Panda's layer order. There is no application CSS stylesheet to maintain alongside recipes.

The original handoff files in `docs/design/kasuri-ui/` are reference material, not styling build inputs.

## Using recipes

Prefer the shared components for consistent variants and native semantics:

```tsx
import { Button, Pill, Tile } from './design/ui'

<Button visual="primary" onClick={save}>Save workflow</Button>
<Button visual="ghost" tone="danger" onClick={cancel}>Cancel run</Button>
<Pill status="waiting" />
<Tile kind="HTTP Request" />
```

For a multi-part component, edit its `defineSlotRecipe` in `theme/recipes/` and use the corresponding classes through `styles`. Use recipe variants for discrete states; use `cx` from `styled-system/css` when combining classes. Panda's generated `css()` utility is available for one-off styles. Use token names or references such as `{colors.accent}` rather than duplicating palette values.

Keep continuously changing values, such as canvas coordinates, progress percentages and a tile's requested size, as inline values. API-derived status and category colours should use the finite recipe variants.

## Development and builds

Use Node 22 or newer and run `pnpm install`. The frontend's `prepare` script generates its typed `styled-system` API. That directory is ignored by Git.

`pnpm dev:web` starts Vite with Panda generation and hot updates. `pnpm build:web` explicitly runs code generation before TypeScript, then Vite generates the production stylesheet. This also works when building from a fresh checkout without generated files. `pnpm --filter web styles:check` generates the standalone Panda output for inspection.

No generated styling code needs to be committed or manually copied into a release.
