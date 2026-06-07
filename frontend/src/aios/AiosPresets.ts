import type { LayoutMode, PanelType, PresetName } from '@/aios/store/aios'

// ── Preset slot definition ─────────────────────────────────────────────────────

export interface PresetSlot {
  component: string
  title: string
  icon?: string
  type?: PanelType
  /** Snap zone or 'floating' */
  snapZone?: LayoutMode
  /** For floating panels: explicit geometry as fraction of viewport */
  xFrac?: number
  yFrac?: number
  wFrac?: number
  hFrac?: number
  /** Absolute pixel overrides (resolved by applyPreset) */
  x?: number
  y?: number
  w?: number
  h?: number
  /** Only open this slot on desktop/large tiers */
  desktopOnly?: boolean
}

export interface PresetDef {
  name: PresetName
  label: string
  description: string
  /** Visual thumbnail string (displayed in picker) */
  thumbnail: string
  slots: PresetSlot[]
}

// ── Preset definitions ─────────────────────────────────────────────────────────

export const PRESETS: Record<PresetName, PresetDef> = {

  'ai-assist': {
    name: 'ai-assist',
    label: 'AI Assist',
    description: 'Resources left, Odysseus right',
    thumbnail:
      '┌──────┬──────┐\n' +
      '│      │      │\n' +
      '│ App  │  AI  │\n' +
      '│      │      │\n' +
      '└──────┴──────┘',
    slots: [
      {
        component: 'dashboard',
        title: 'Dashboard',
        icon: '',
        snapZone: 'left-half',
      },
      {
        component: 'ai',
        title: 'Odysseus',
        icon: '🧠',
        type: 'app',
        snapZone: 'right-half',
      },
    ],
  },

  'debug': {
    name: 'debug',
    label: 'Debug',
    description: 'App + Odysseus + Logs + Metrics in four quarters',
    thumbnail:
      '┌──────┬──────┐\n' +
      '│ App  │  AI  │\n' +
      '├──────┼──────┤\n' +
      '│ Logs │ Mtrc │\n' +
      '└──────┴──────┘',
    slots: [
      {
        component: 'dashboard',
        title: 'Dashboard',
        snapZone: 'top-left',
      },
      {
        component: 'ai',
        title: 'Odysseus',
        type: 'app',
        snapZone: 'top-right',
      },
      {
        component: 'timeline',
        title: 'Timeline',
        snapZone: 'bottom-left',
      },
      {
        component: 'diagnostics',
        title: 'Diagnostics',
        snapZone: 'bottom-right',
      },
    ],
  },

  'vm': {
    name: 'vm',
    label: 'VM Manager',
    description: 'VMs center, Terminal right',
    thumbnail:
      '┌────────────┬───┐\n' +
      '│            │   │\n' +
      '│    VMs     │ T │\n' +
      '│            │   │\n' +
      '└────────────┴───┘',
    slots: [
      {
        component: 'vms',
        title: 'VMs',
        snapZone: 'floating',
        // 70% wide, 88% of canvas height, centered — resolved in applyPreset via xFrac
        xFrac: 0.02,
        yFrac: 0.0,
        wFrac: 0.68,
        hFrac: 0.92,
      },
      {
        component: 'terminal',
        title: 'Terminal',
        snapZone: 'right-half',
      },
    ],
  },

  'android': {
    name: 'android',
    label: 'Android Dev',
    description: 'Containers left, Terminal right, Odysseus floating (desktop)',
    thumbnail:
      '┌──────────┬──────┐\n' +
      '│          │      │\n' +
      '│  Contnr  │ Term │\n' +
      '│          │      │\n' +
      '└──────────┴──────┘',
    slots: [
      {
        component: 'containers',
        title: 'Containers',
        snapZone: 'left-half',
      },
      {
        component: 'terminal',
        title: 'Terminal',
        snapZone: 'right-half',
      },
      {
        component: 'ai',
        title: 'Odysseus',
        type: 'app',
        snapZone: 'floating',
        // Small floating window top-right
        xFrac: 0.68,
        yFrac: 0.0,
        wFrac: 0.30,
        hFrac: 0.40,
        desktopOnly: true,
      },
    ],
  },
}

// ── Preset metadata list (for UI rendering) ────────────────────────────────────

export const PRESET_LIST: PresetDef[] = Object.values(PRESETS)
