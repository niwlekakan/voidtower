// Synthetic barrel for design-sync — isolatable UI components only.
// NOT a source file: lives in gitignored .ds-sync/, regenerated each sync.
export { default as Button } from '../src/components/ui/Button'
export { default as StatusBadge } from '../src/components/ui/StatusBadge'
export { default as AiBadge } from '../src/components/ui/AiBadge'
export type { AiBadgeProps, AiBadgeConfig, AiLevel } from '../src/components/ui/AiBadge'
export { default as MetricCard } from '../src/components/ui/MetricCard'
export { default as MetricChart } from '../src/components/ui/MetricChart'

// Easy adds — no app-store/API coupling
export { default as ChangePlanModal } from '../src/components/ui/ChangePlanModal'
export type { ChangePlan, ChangePlanChange } from '../src/components/ui/ChangePlanModal'
export { default as ConfirmDialog } from '../src/components/ui/ConfirmDialog'
export { default as LogViewer } from '../src/components/ui/LogViewer'

// Store/API-coupled, verified to bundle cleanly (import.meta.env is a build
// warning only — collapses to undefined, doesn't crash) — see NOTES.md
export { default as AnimatedBackground } from '../src/components/ui/AnimatedBackground'
export { default as AppEmbedOverlay } from '../src/components/ui/AppEmbedOverlay'
export { default as CommandPalette } from '../src/components/ui/CommandPalette'
export { default as ForcePasswordChange } from '../src/components/ui/ForcePasswordChange'
export { default as MiniTerminal } from '../src/components/ui/MiniTerminal'
export { default as NotificationToasts } from '../src/components/ui/NotificationToasts'
export { default as SendToOdysseus } from '../src/components/ui/SendToOdysseus'
export { default as ThemeEditor } from '../src/components/ui/ThemeEditor'

// Store hooks re-exported (lowercase — never mistaken for components) so
// authored previews can prime global state before rendering
export { useThemeStore } from '../src/store/theme'
export { useCmdPaletteStore } from '../src/store/cmdpalette'
export { useEmbedStore } from '../src/store/embedStore'
export { useNotificationStore, notify } from '../src/store/notifications'

// Re-exported through THIS bundle (not imported separately by a preview) so
// CommandPalette's internal useNavigate() shares the same Router context —
// react-router-dom imported a second time from a separate module graph
// creates a distinct context object and useNavigate() throws.
export { MemoryRouter } from 'react-router-dom'
