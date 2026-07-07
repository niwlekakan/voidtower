import React from 'react'
import { StyleSheet, Text } from 'react-native'
import { colors, type as t } from '../theme/tokens'

export function SectionLabel({ children }: { children: React.ReactNode }) {
  return <Text style={styles.label}>{children}</Text>
}

const styles = StyleSheet.create({
  label: {
    ...t.sectionLabel,
    color: colors.textMuted,
    marginBottom: 8,
    textTransform: 'uppercase',
  },
})
