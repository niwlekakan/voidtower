import React from 'react'
import { StyleSheet, View, ViewProps } from 'react-native'
import { colors, radii, spacing } from '../theme/tokens'

export function Card({ style, ...props }: ViewProps) {
  return <View style={[styles.card, style]} {...props} />
}

const styles = StyleSheet.create({
  card: {
    backgroundColor: colors.cardBg,
    borderColor: colors.cardBorder,
    borderWidth: 1,
    borderRadius: radii.card,
    padding: spacing.cardPadding,
  },
})
