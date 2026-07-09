// Transcribed verbatim from the design handoff
// (design_handoff_voidtower_mobile/README.md — "Design Tokens" section).
// Do not invent values here; if a new token is needed, add it to the handoff first.

export const colors = {
  screenBg: '#050509',
  cardBg: 'rgba(17,19,29,0.96)',
  cardBorder: '#25283a',
  accentPurple: '#8b5cf6',
  accentPurpleDark: '#7c3aed',
  accentCyan: '#06b6d4',
  accentGreen: '#39ff88',
  accentAmber: '#f59e0b',
  accentRed: '#ef4444',
  spotifyGreen: '#1db954',
  textPrimary: '#f4f7ff',
  textSecondary: '#a8b0c3',
  textMuted: '#687086',
  separator: '#25283a',
  inactiveDot: '#3d4160',
  overlayBg: 'rgba(2,2,5,0.72)',
  navGradientStart: 'transparent',
  navGradientEnd: 'rgba(5,5,9,0.88)',
} as const

export const fonts = {
  mono: 'JetBrainsMono_600SemiBold',
  monoRegular: 'JetBrainsMono_400Regular',
  monoMedium: 'JetBrainsMono_500Medium',
  monoBold: 'JetBrainsMono_700Bold',
  system: undefined, // falls back to the platform system font
} as const

export const type = {
  clock: { fontSize: 24, fontWeight: '700' as const },
  sectionLabel: { fontSize: 11, fontWeight: '700' as const, letterSpacing: 1.5 },
  cardTitle: { fontSize: 15, fontWeight: '600' as const },
  body: { fontSize: 12, fontWeight: '400' as const },
  statValue: { fontSize: 22, fontWeight: '700' as const },
  badge: { fontSize: 10, fontWeight: '700' as const, letterSpacing: 1 },
  statusBarTime: { fontSize: 13, fontWeight: '600' as const, letterSpacing: 0.5 },
  navLabel: { fontSize: 8, fontWeight: '600' as const, letterSpacing: 0.5 },
}

export const spacing = {
  cardPadding: 14,
  sectionLabelMargin: 8,
  sectionGap: 16,
}

export const radii = {
  card: 16,
  sheetTop: 28,
  sceneButton: 14,
  pill: 999,
  deviceIcon: 11,
  fab: 999,
}

export const shadows = {
  // React Native has no CSS box-shadow; these map to the handoff's glows via
  // per-platform shadow/elevation props where used.
  accentCardGlow: { shadowColor: colors.accentPurple, shadowOpacity: 0.65, shadowRadius: 26, elevation: 8 },
  mediaArtGlow: { shadowColor: colors.accentPurple, shadowOpacity: 0.55, shadowRadius: 36, elevation: 10 },
}

export const deviceTypeColors: Record<string, string> = {
  tv: colors.accentPurple,
  speaker: colors.accentPurple,
  light: colors.accentAmber,
  wled: colors.accentAmber,
  climate: colors.accentRed,
  lock: colors.accentGreen,
  cover: colors.accentCyan,
  plug: colors.accentCyan,
  camera: colors.accentPurple,
  vacuum: colors.accentCyan,
  server: colors.accentCyan,
  nas: colors.accentCyan,
  computer: colors.accentCyan,
  phone: colors.accentCyan,
}

export const severityColors: Record<string, string> = {
  info: colors.accentCyan,
  warning: colors.accentAmber,
  critical: colors.accentRed,
}
