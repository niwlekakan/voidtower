# VoidTower UI — design-sync notes

## Repo context
- VoidTower is an **application** (not a standalone design system library).
- Components live in `frontend/src/components/ui/` alongside app-specific code.
- All converter commands run from the `frontend/` directory.
- `--node-modules frontend/node_modules` for the converter.
- Package is `voidtower-frontend`; there is no root-level `package.json`.

## Component scope
First sync covers 5 clean, isolatable components:
- Button, StatusBadge, AiBadge, MetricCard, MetricChart

**Excluded — app-level imports that break bundling:**
- `TagPill.tsx` — file-level `import { api } from '@/api/client'`; `api/client.ts` uses `import.meta.env.VITE_API_BASE` (Vite-specific, not bundlable by esbuild in synth-entry mode). To include in a future sync: extract `TagPill` to its own file without the `TagPopover`/`TagSelector` exports.
- `UiModeToggle.tsx` — imports `@/store/theme` which transitively imports `../aios/store/aios` (large AIOS Zustand store). To include: mock the Zustand store via `cfg.provider` or extract the toggle to a prop-driven component.

## CSS
- Tokens are CSS custom properties defined in `src/styles/global.css`.
- Styling idiom: Tailwind utility classes for layout/spacing + inline `style={}` props for themed colors via `var(--token-*)`.
- Build CSS before converter: `npx tailwindcss -i src/styles/global.css -o .ds-sync/vt-styles.css --minify` (from `frontend/`).
- All path-valued config fields (`srcDir`, `tsconfig`, `componentSrcMap` entries, `cssEntry`) are resolved relative to **`.design-sync/`'s own directory**, not the process CWD — every one needs a `../` prefix to reach `frontend/src/...` or `frontend/.ds-sync/...`. `readmeHeader` is the one exception: it resolves relative to the config home (`frontend/`, the directory *containing* `.design-sync/`), so it's written without a `../` prefix (e.g. `".design-sync/conventions.md"`).
- No published `dist/` entry exists (VoidTower is an app) — synth-entry mode needs a synthetic barrel: `.ds-sync/entry.ts` re-exporting just the scoped components, passed via `--entry .ds-sync/entry.ts`.

## Playwright/chromium pinning (for the render check)
- The local chromium cache dir (`~/.cache/ms-playwright/chromium-<rev>/`) pins an exact playwright release — the repo carries no pinned version, so match by reading `node_modules/playwright-core/browsers.json`'s chromium revision across candidate versions until one matches the cached `<rev>`. For this machine, cache revision 1223 → `playwright-core@1.60.0` / `playwright@1.60.0`. Install with `PLAYWRIGHT_SKIP_BROWSER_DOWNLOAD=1` to avoid re-fetching the already-cached browser binary.

## AiBadge gotcha
- The `ready` level uses near-white/translucent tones (`rgba(255,255,255,...)`) designed to sit on the app's dark `--bg-card` surface — invisible against a white preview background. The authored preview (`.design-sync/previews/AiBadge.tsx`) wraps all stories in a `background: var(--bg-card)` container for this reason; don't drop that wrapper on a re-author.

## MetricCard gotcha
- The `Dashboard` story (4-card grid composition) is wider than the default 2-column preview grid cell, triggering `[GRID_OVERFLOW]`. Fixed via `cfg.overrides.MetricCard: {"cardMode": "column"}` (already in config.json) — don't remove without re-checking the render check.

## Re-sync risks
- Tailwind CSS must be recompiled before each sync (`.ds-sync/vt-styles.css` is gitignored).
- New components added to `src/components/ui/` may import from `@/api/`, `@/store/`, or `@/aios/` — check before adding to `componentSrcMap`.
- `AiBadge.tsx` re-exports `getAiBadgeConfig` (a non-component export); the converter will include it as a bundle export. Prune with `componentSrcMap: {"getAiBadgeConfig": null}` if it causes `[ZERO_MATCH]` or appears as a component card.
- The chromium/playwright version pin above is machine-local — a fresh clone or CI box will need to re-derive the matching version (or already have a compatible one cached).
- `MetricChart` previews need an explicit pixel-sized wrapper (`width`/`height` on the parent div) — `recharts`' `ResponsiveContainer` can't size itself from an unconstrained parent and would otherwise render blank.

## Known render warns
- None currently — render check is clean (5/5) as of the first sync.
