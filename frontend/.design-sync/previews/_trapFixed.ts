// Shared helper, not a component preview (no PascalCase default export, so
// the converter ignores this file as a story source).
//
// The emitted card's single-mode wrapper (.ds-single) sits in a
// `transform: translateZ(0)` box specifically so it becomes the CSS
// containing block for `position: fixed`/`absolute` descendants — that's
// how a component using `fixed inset-0` (a real app-level modal/overlay)
// gets trapped inside the card instead of escaping to the host page's
// viewport. But .ds-single itself has no explicit size, and an
// out-of-flow-positioned-only child doesn't contribute to its parent's
// auto height, so the box collapses to ~0 and the trapped content renders
// cropped. Giving .ds-single an explicit size (matching the component's
// own cfg.overrides.<Name>.viewport) fixes it — call this once at preview
// module scope with the same WxH used in config.json.
export function trapFixedAt(width: number, height: number) {
  const style = document.createElement('style')
  style.textContent = `.ds-single{width:${width}px;height:${height}px}`
  document.head.appendChild(style)
}
