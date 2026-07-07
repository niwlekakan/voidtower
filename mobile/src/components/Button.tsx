import React from 'react'
import { ActivityIndicator, Pressable, StyleSheet, Text } from 'react-native'
import { colors, radii } from '../theme/tokens'

interface ButtonProps {
  title: string
  onPress: () => void
  loading?: boolean
  disabled?: boolean
  variant?: 'primary' | 'secondary'
}

export function Button({ title, onPress, loading, disabled, variant = 'primary' }: ButtonProps) {
  const isPrimary = variant === 'primary'
  return (
    <Pressable
      onPress={onPress}
      disabled={disabled || loading}
      style={({ pressed }) => [
        styles.base,
        isPrimary ? styles.primary : styles.secondary,
        (disabled || loading) && styles.disabled,
        pressed && styles.pressed,
      ]}
    >
      {loading
        ? <ActivityIndicator color={colors.textPrimary} />
        : <Text style={[styles.text, !isPrimary && { color: colors.textPrimary }]}>{title}</Text>}
    </Pressable>
  )
}

const styles = StyleSheet.create({
  base: {
    borderRadius: radii.card,
    paddingVertical: 14,
    alignItems: 'center',
    justifyContent: 'center',
  },
  primary: { backgroundColor: colors.accentPurple },
  secondary: { backgroundColor: 'transparent', borderWidth: 1, borderColor: colors.cardBorder },
  disabled: { opacity: 0.5 },
  pressed: { opacity: 0.85 },
  text: { color: '#fff', fontSize: 15, fontWeight: '700' },
})
