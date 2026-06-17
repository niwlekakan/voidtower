## Conventions

VoidTower's UI is a dark, terminal-adjacent control-plane interface. Components style themselves with **CSS custom property tokens** (defined in `:root`, themeable via `[data-theme="…"]` overrides) — never hardcoded hex values. Layout and spacing use Tailwind utility classes; color, border, and background come from `var(--token-name)` in inline `style` props.

- **Surfaces are dark by default.** `--bg-root` / `--bg-panel` / `--bg-card` / `--bg-elevated` step from darkest (page) to lightest (nested card). Compose new mockups on one of these, not white — some components (e.g. the lowest-emphasis `AiBadge` "ready" level) use low-contrast near-white tones that assume a dark surface and disappear on light backgrounds.
- **Semantic accents**, not raw colors: `--accent-primary` (violet, primary actions/highlights), `--accent-secondary` (cyan), `--accent-success` (green), `--accent-warning` (amber), `--accent-danger` (red). Status/state indicators (badges, dots, chart lines) pick one of these by meaning, not by picking a color directly.
- **Monospace for data**, sans for everything else — `MetricCard`'s value uses `font-mono`; labels/body text don't.
- **Buttons** (`Button`) carry a `variant` (`primary`/`secondary`/`danger`/`ghost`) × `size` (`sm`/`md`/`lg`) matrix; `loading` swaps in a spinner and disables the control. `secondary` is the default — `primary` is reserved for the one clear call-to-action in a given view.
- **Compact modes** exist on space-constrained badges (`AiBadge`'s `compact` drops the label, keeping only the icon) for use in dense toolbars/panels.

This conventions file is prepended to the generated README so a design agent mocking up new VoidTower screens reuses the same token vocabulary instead of inventing new colors.
