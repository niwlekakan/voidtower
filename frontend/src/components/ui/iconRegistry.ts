import {
  LayoutDashboard, Server, Container, Package, Bell, HardDrive, Network, Terminal,
  ClipboardList, Settings, Shield, Lock, BrainCircuit, FolderOpen, Globe, KeyRound,
  History, Flame, Zap, Wifi, Monitor, Tag, ArrowUpCircle, PlugZap, Puzzle, Palette,
  Blocks, Box, Wand2, Cpu, Stethoscope, Bot, MoreHorizontal, Activity, LayoutPanelTop,
} from 'lucide-react'
import type { LucideIcon } from 'lucide-react'

/** Selectable icon set for the nav editor's icon picker, keyed by lucide-react export name. */
export const ICON_REGISTRY: Record<string, LucideIcon> = {
  LayoutDashboard, Server, Container, Package, Bell, HardDrive, Network, Terminal,
  ClipboardList, Settings, Shield, Lock, BrainCircuit, FolderOpen, Globe, KeyRound,
  History, Flame, Zap, Wifi, Monitor, Tag, ArrowUpCircle, PlugZap, Puzzle, Palette,
  Blocks, Box, Wand2, Cpu, Stethoscope, Bot, MoreHorizontal, Activity, LayoutPanelTop,
}

export const ICON_NAMES = Object.keys(ICON_REGISTRY)
