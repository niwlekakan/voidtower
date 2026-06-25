# VoidTower UI — design-sync notes

## Repo context
- VoidTower is an **application** (not a standalone design system library).
- Components live in `frontend/src/components/ui/` alongside app-specific code.
- All converter commands run from the `frontend/` directory.
- `--node-modules frontend/node_modules` for the converter.
- Package is `voidtower-frontend`; there is no root-level `package.json`.

## Component scope
16 components synced as of the second pass:
- First sync (isolatable, no app coupling): Button, StatusBadge, AiBadge, MetricCard, MetricChart
- Second sync — easy adds (no store/API imports): ChangePlanModal, ConfirmDialog, LogViewer
- Second sync — store/API-coupled, verified to bundle cleanly: AnimatedBackground, AppEmbedOverlay, CommandPalette, ForcePasswordChange, MiniTerminal, NotificationToasts, SendToOdysseus, ThemeEditor

**Correction to the original exclusion rationale**: `import.meta.env.VITE_API_BASE` in `api/client.ts` is **not** a hard esbuild failure — confirmed empirically (`esbuild --bundle --format=iife` on a file importing `api/client.ts` exits 0 with only an `[empty-import-meta]` warning; the value just collapses to `undefined`/`''` at runtime). The 8 "blocked" components above all import `@/api/client` directly or transitively and bundle fine. The real per-component blockers were narrower runtime issues (see below), not the import.meta warning.

To make the 8 above render correctly without their backend, the entry barrel (`.ds-sync/entry.ts`) also re-exports the relevant Zustand store hooks (`useThemeStore`, `useCmdPaletteStore`, `useEmbedStore`, `useNotificationStore`, `notify`) and `MemoryRouter` (re-exported from `react-router-dom` through the SAME bundle — see CommandPalette gotcha below) so authored previews can prime global state / wrap in a router before rendering. Components that return `null` by default (`CommandPalette` when `open:false`, `NotificationToasts` when empty, `AppEmbedOverlay` when `app:null`) need this priming or they show the floor card.

**Still excluded — app-level imports that break bundling:**
- `TagPill.tsx` — same `@/api/client` import as the 8 above, BUT also pulls in `TagPopover`/`TagSelector` from the same file; not re-tested this pass. To include: extract `TagPill` to its own file, or just re-test — the original exclusion reason (import.meta.env) is now known to be wrong.
- `UiModeToggle.tsx` — imports `@/store/theme` (now proven clean, see above) — also not re-tested this pass, may well be includable now. Re-test before excluding again.
- `layout/AppLayout.tsx`, `layout/Sidebar.tsx`, `layout/TopBar.tsx` — deliberately out of scope: app-shell/navigation chrome tied to routing and global state, not reusable design-system components (user decision, not a bundling limitation).

## CSS
- Tokens are CSS custom properties defined in `src/styles/global.css`.
- Styling idiom: Tailwind utility classes for layout/spacing + inline `style={}` props for themed colors via `var(--token-*)`.
- Build CSS before converter: `npx tailwindcss -i src/styles/global.css -o .ds-sync/vt-styles.css --minify` (from `frontend/`).
- All path-valued config fields (`srcDir`, `tsconfig`, `componentSrcMap` entries, `cssEntry`) are resolved relative to **`.design-sync/`'s own directory**, not the process CWD — every one needs a `../` prefix to reach `frontend/src/...` or `frontend/.ds-sync/...`. `readmeHeader` is the one exception: it resolves relative to the config home (`frontend/`, the directory *containing* `.design-sync/`), so it's written without a `../` prefix (e.g. `".design-sync/conventions.md"`).
- No published `dist/` entry exists (VoidTower is an app) — synth-entry mode needs a synthetic barrel: `.ds-sync/entry.ts` re-exporting just the scoped components, passed via `--entry .ds-sync/entry.ts`.

## Playwright/chromium pinning (for the render check)
- The local chromium cache dir (`~/.cache/ms-playwright/chromium-<rev>/`) pins an exact playwright release — the repo carries no pinned version, so match by reading `node_modules/playwright-core/browsers.json`'s chromium revision across candidate versions until one matches the cached `<rev>`. **This drifts across sessions as the machine's global playwright cache gets updated by unrelated work** — don't trust a previously-recorded version pin without re-verifying. As of the second sync, cache revision 1228 → `playwright-core@1.61.0` / `playwright@1.61.0` (found by fetching `https://raw.githubusercontent.com/microsoft/playwright/v<X.Y.Z>/packages/playwright-core/browsers.json` for candidate versions until the chromium revision matched). Install with `PLAYWRIGHT_SKIP_BROWSER_DOWNLOAD=1` to avoid re-fetching the already-cached browser binary.

## AiBadge gotcha
- The `ready` level uses near-white/translucent tones (`rgba(255,255,255,...)`) designed to sit on the app's dark `--bg-card` surface — invisible against a white preview background. The authored preview (`.design-sync/previews/AiBadge.tsx`) wraps all stories in a `background: var(--bg-card)` container for this reason; don't drop that wrapper on a re-author.
- **Same gotcha hit `SendToOdysseus` and `ThemeEditor` in the second sync** — both use low-contrast accent tones assuming a dark host surface. Same fix: wrap the preview in `background: var(--bg-card)` (SendToOdysseus) or `var(--bg-panel)` (ThemeEditor, since it's a settings-panel-sized surface). **Any future component whose styling reads `--accent-*-subtle` or similar low-contrast tokens needs this wrapper by default** — check on a white background first, don't assume it's fine.

## MetricCard gotcha
- The `Dashboard` story (4-card grid composition) is wider than the default 2-column preview grid cell, triggering `[GRID_OVERFLOW]`. Fixed via `cfg.overrides.MetricCard: {"cardMode": "column"}` (already in config.json) — don't remove without re-checking the render check.
- **Same fix applied to `ThemeEditor`** (`cardMode: "column"`) — it's a full settings panel, wider than a grid cell.

## `.ds-single` containing-block bug (cardMode: "single" + `position:fixed`/`absolute` components)
- **This is the big one — read before authoring any future overlay/modal preview.** `lib/emit.mjs`'s single-mode wrapper (`<div class="ds-single">`) has `transform: translateZ(0)` with NO explicit width/height. Per the CSS spec, any transformed ancestor becomes the containing block for `position: fixed` (and `absolute`) descendants — this is intentional (it's how the converter traps an app's `fixed inset-0` modal inside the card instead of letting it escape to the host page's real viewport). **But because `.ds-single` has no explicit size, and out-of-flow (fixed/absolute) children don't contribute to their containing block's auto-height, `.ds-single` collapses to ~0 height.** A `position:fixed; inset:0; display:flex; align-items:center` modal centered inside a collapsed container renders with its TOP cropped off — confirmed by direct DOM measurement (`getBoundingClientRect()` on the `.fixed` element returned `height: 32` instead of the expected full viewport height) and by screenshot (modal content starts mid-way through, header/first rows missing).
  - **This is a real bug in the bundled `lib/emit.mjs`, not a config mistake** — `lib/emit.mjs`/`lib/bundle.mjs` are explicitly off-limits to fork per the skill's own rules ("don't fork those; use config overrides instead"), and there's no `cfg.*` override for this specific case.
  - **The sanctioned fix** (no protected files touched): inject a tiny `<style>` rule from inside the OWNED preview `.tsx` itself, at module scope, giving `.ds-single` an explicit size matching the component's own `cfg.overrides.<Name>.viewport`. CSS continues to match elements created after the rule is inserted, so this works even though `.ds-single` doesn't exist yet when the preview module's top-level code runs (the emit template's own inline `<script>` creates it afterward, from a later `<script>` tag in the same document).
  - Shared helper: `.design-sync/previews/_trapFixed.ts` exports `trapFixedAt(width, height)` — NOT a component (no PascalCase default export, ignored by the converter's per-component file lookup). Call it once at the top of any preview whose component uses `position: fixed` or `position: absolute` internally, with the SAME width/height as that component's `cfg.overrides` viewport.
  - Applied to: `ChangePlanModal` (560×680), `ConfirmDialog` (460×340), `ForcePasswordChange` (420×640), `CommandPalette` (560×440), `NotificationToasts` (360×220).
  - **NOT needed** for `AnimatedBackground` (fixed, but its `<canvas>` has explicit HTML width/height attributes set via JS from `window.innerWidth/innerHeight` — a canvas with intrinsic attributes falls back to its own size when a CSS percentage height resolves to `auto` against an indefinite-height parent, so it renders correctly without the helper) or `AppEmbedOverlay` (uses `position:absolute`, not `fixed` — my own preview's `position:relative` sized wrapper div is a NEARER ancestor than `.ds-single` and correctly wins as the containing block for absolute, unlike fixed which only transformed ancestors can intercept).
  - **Don't just throw `trapFixedAt` at every overlay reflexively** — verify with a real screenshot first; some genuinely don't need it (see above), and adding it costs nothing but isn't free to maintain (the WxH must stay in sync with `cfg.overrides`).
  - **Even after the fix, leave comfortable margin** in the declared viewport vs. the component's natural content size — `CommandPalette` initially used 480×420 (its `max-w-md` panel is 448px, leaving only 32px total margin) and ended up 1px past the right edge with asymmetric centering; bumped to 560×440. `ConfirmDialog` had the same near-miss (384px panel in 420px viewport); bumped to 460×340. Rule of thumb: viewport width should exceed the component's max content width by at least ~80-100px, not just a few px.

## Re-sync risks
- Tailwind CSS must be recompiled before each sync (`.ds-sync/vt-styles.css` is gitignored).
- New components added to `src/components/ui/` may import from `@/api/`, `@/store/`, or `@/aios/` — check before adding to `componentSrcMap`. As of the second sync, **don't assume these imports block bundling** (see "Correction" above under Component scope) — check what the import is actually used FOR (synchronous required-prop usage, router hooks needing a provider, etc.), not just whether it exists.
- `AiBadge.tsx` re-exports `getAiBadgeConfig` (a non-component export); the converter will include it as a bundle export. Prune with `componentSrcMap: {"getAiBadgeConfig": null}` if it causes `[ZERO_MATCH]` or appears as a component card.
- The chromium/playwright version pin above is machine-local and drifts — re-derive it each sync, don't trust the last-recorded version.
- `MetricChart` previews need an explicit pixel-sized wrapper (`width`/`height` on the parent div) — `recharts`' `ResponsiveContainer` can't size itself from an unconstrained parent and would otherwise render blank.
- Components reading Zustand store state that defaults to "empty"/`null` (`CommandPalette`, `NotificationToasts`, `AppEmbedOverlay`) need their preview to prime the store at module scope before rendering, or they show the floor card. Only author ONE export per such component — the store is a global singleton shared by every cell mounted in the same page/iframe, so two exports setting different state would fight each other if ever rendered simultaneously in a future grid view.
- `CommandPalette` calls `useNavigate()` — needs `MemoryRouter`, but `MemoryRouter` must be imported from the SAME bundle as `CommandPalette` (re-exported via `.ds-sync/entry.ts`), not imported separately by the preview from `react-router-dom` directly — two separate esbuild module graphs create two different React Context objects, and `useNavigate()` throws "may be used only in the context of a Router" even though a `MemoryRouter` IS present, just the wrong one.
- See the `.ds-single` containing-block section above before authoring any new preview for a component that uses `position: fixed`/`absolute` internally.

## Known render warns
- None — render check is clean (16/16) as of the second sync; 0 bad/thin/variantsIdentical across 3 fix iterations.

## conventions.md validation (second sync)
- Re-checked all claimed tokens/props against the fresh build — still accurate, nothing rewritten.
- **Found incidentally, not fixed here**: `ChangePlanModal.tsx`'s `RISK_COLOR.high` reads `var(--accent-error)`, but the real, defined token everywhere else in the app (and the one `conventions.md` documents) is `--accent-danger`. `--accent-error` isn't defined in `vt-styles.css` at all — this looks like a typo in VoidTower's actual source, not a design-sync issue. Out of scope for this sync to fix; worth a heads-up to whoever owns `ChangePlanModal.tsx`.
