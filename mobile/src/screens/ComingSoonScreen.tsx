import React from 'react'
import { SafeAreaView, StyleSheet, Text } from 'react-native'
import { colors } from '../theme/tokens'

const COPY: Record<string, string> = {
  Media: 'Jellyfin, Spotify, and casting arrive once those integrations are wired up.',
  Devices: 'Device discovery and control arrive alongside the Home Assistant integration.',
  Audio: 'Whole-home audio zones arrive once Snapcast/WLED are wired up.',
}

/**
 * Shared placeholder for the remaining phase-two tabs (Media/Devices/Audio).
 * These all depend on backend integrations (Home Assistant, Jellyfin,
 * Spotify, Snapcast/WLED) that don't exist yet — see plan Part C. Home got
 * its own real screen (HomeScreen) in the mobile UI restructure.
 */
export function ComingSoonScreen({ tab }: { tab: keyof typeof COPY }) {
  return (
    <SafeAreaView style={styles.screen}>
      <Text style={styles.title}>{tab}</Text>
      <Text style={styles.badge}>COMING IN PHASE TWO</Text>
      <Text style={styles.body}>{COPY[tab]}</Text>
    </SafeAreaView>
  )
}

const styles = StyleSheet.create({
  screen: { flex: 1, backgroundColor: colors.screenBg, alignItems: 'center', justifyContent: 'center', padding: 32 },
  title: { color: colors.textPrimary, fontSize: 22, fontWeight: '700', marginBottom: 10 },
  badge: {
    color: colors.accentPurple,
    fontSize: 10,
    fontWeight: '700',
    letterSpacing: 1.5,
    marginBottom: 16,
  },
  body: { color: colors.textSecondary, fontSize: 14, textAlign: 'center', lineHeight: 20 },
})
