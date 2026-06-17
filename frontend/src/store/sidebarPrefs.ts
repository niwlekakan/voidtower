import { create } from 'zustand'
import { persist } from 'zustand/middleware'

export type SidebarAnimationStyle = 'slide' | 'fade' | 'squeeze' | 'stagger' | 'flip' | 'bounce'
export type SidebarPlacement = 'left' | 'right' | 'top' | 'bottom'

interface SidebarPrefsState {
  animation: SidebarAnimationStyle
  setAnimation: (style: SidebarAnimationStyle) => void
  placement: SidebarPlacement
  setPlacement: (placement: SidebarPlacement) => void
  autoHide: boolean
  setAutoHide: (autoHide: boolean) => void
}

export const useSidebarPrefsStore = create<SidebarPrefsState>()(
  persist(
    (set) => ({
      animation: 'slide',
      setAnimation: (animation) => set({ animation }),
      placement: 'left',
      setPlacement: (placement) => set({ placement }),
      autoHide: false,
      setAutoHide: (autoHide) => set({ autoHide }),
    }),
    { name: 'vt-sidebar-prefs' },
  ),
)

export const SIDEBAR_PLACEMENT_OPTIONS: { value: SidebarPlacement; label: string; description: string }[] = [
  { value: 'left',   label: 'Left sidebar',   description: 'Default — docked to the left edge.' },
  { value: 'right',  label: 'Right sidebar',  description: 'Docked to the right edge.' },
  { value: 'top',    label: 'Top bar',        description: 'Horizontal bar across the top.' },
  { value: 'bottom', label: 'Bottom bar',     description: 'Horizontal bar across the bottom.' },
]

export const SIDEBAR_ANIMATION_OPTIONS: { value: SidebarAnimationStyle; label: string; description: string }[] = [
  { value: 'slide',   label: 'Slide & Fade',     description: 'Labels slide in from the left as the sidebar expands.' },
  { value: 'fade',    label: 'Simple Fade',      description: 'Labels and headers crossfade in place — no movement.' },
  { value: 'squeeze', label: 'Spring Squeeze',   description: 'Sidebar width bounces with a springy overshoot.' },
  { value: 'stagger', label: 'Staggered Reveal', description: 'Nav items cascade in one after another.' },
  { value: 'flip',    label: '3D Flip',          description: 'Labels and headers flip into view on a 3D edge.' },
  { value: 'bounce',  label: 'Bouncy Drop',      description: 'Items drop down and settle with a springy bounce.' },
]
