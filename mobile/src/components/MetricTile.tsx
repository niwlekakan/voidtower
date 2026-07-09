import React from 'react'
import { StyleSheet, Text, View } from 'react-native'
import { Card } from './Card'
import { colors } from '../theme/tokens'

interface Props {
  label: string
  value: string
  sub: string
  pct: number // 0-100
  color: string
}

export function MetricTile({ label, value, sub, pct, color }: Props) {
  return (
    <Card style={styles.card}>
      <Text style={styles.label}>{label}</Text>
      <Text style={[styles.value, { color }]}>{value}</Text>
      <View style={styles.track}>
        <View style={[styles.fill, { width: `${Math.min(100, Math.max(0, pct))}%`, backgroundColor: color }]} />
      </View>
      <Text style={styles.sub}>{sub}</Text>
    </Card>
  )
}

const styles = StyleSheet.create({
  card: { flex: 1, gap: 6 },
  label: { color: colors.textMuted, fontSize: 10, fontWeight: '700', letterSpacing: 1 },
  value: { fontSize: 22, fontWeight: '700' },
  track: { height: 4, borderRadius: 2, backgroundColor: colors.separator, overflow: 'hidden' },
  fill: { height: 4, borderRadius: 2 },
  sub: { color: colors.textMuted, fontSize: 10 },
})
