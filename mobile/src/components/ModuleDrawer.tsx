import BottomSheet, { BottomSheetFlatList, BottomSheetBackdrop } from '@gorhom/bottom-sheet'
import React, { useEffect, useMemo, useRef, useState } from 'react'
import { ActivityIndicator, StyleSheet, Text, View } from 'react-native'
import { api } from '../api/client'
import { colors, radii } from '../theme/tokens'

export interface ModuleDef {
  id: string
  label: string
  endpoint: string
  /** Some modules (Terminal, Updates) only have a partial read-summary in phase one. */
  summaryOnly?: boolean
}

interface Props {
  module: ModuleDef | null
  onClose: () => void
}

/** Best-effort: find the first array in the response and treat it as the row list. */
function extractRows(data: unknown): Record<string, unknown>[] {
  if (Array.isArray(data)) return data as Record<string, unknown>[]
  if (data && typeof data === 'object') {
    for (const value of Object.values(data as Record<string, unknown>)) {
      if (Array.isArray(value)) return value as Record<string, unknown>[]
    }
  }
  return []
}

const TITLE_KEYS = ['name', 'title', 'display_name', 'domain', 'username', 'filename', 'project_name', 'hostname', 'id']
const STATUS_KEYS = ['active_state', 'state', 'status', 'severity', 'enabled', 'available']

function rowTitle(row: Record<string, unknown>): string {
  for (const key of TITLE_KEYS) {
    if (typeof row[key] === 'string') return row[key] as string
  }
  return 'Item'
}

function rowMeta(row: Record<string, unknown>): string {
  return Object.entries(row)
    .filter(([k]) => !TITLE_KEYS.includes(k))
    .slice(0, 3)
    .map(([k, v]) => `${k}: ${typeof v === 'object' ? JSON.stringify(v) : String(v)}`)
    .join(' · ')
}

function rowDotColor(row: Record<string, unknown>): string {
  const raw = STATUS_KEYS.map((k) => row[k]).find((v) => v !== undefined)
  const s = String(raw).toLowerCase()
  if (['active', 'running', 'true', 'ok', 'up', 'success', 'available'].includes(s)) return colors.accentGreen
  if (['warning', 'degraded'].includes(s)) return colors.accentAmber
  if (['failed', 'critical', 'false', 'down', 'error'].includes(s)) return colors.accentRed
  return colors.inactiveDot
}

export function ModuleDrawer({ module, onClose }: Props) {
  const sheetRef = useRef<BottomSheet>(null)
  const [rows, setRows] = useState<Record<string, unknown>[]>([])
  const [loading, setLoading] = useState(false)
  const [error, setError] = useState<string | null>(null)

  const snapPoints = useMemo(() => ['70%'], [])

  useEffect(() => {
    if (!module) return
    sheetRef.current?.snapToIndex(0)
    setLoading(true)
    setError(null)
    api.moduleSummary(module.endpoint)
      .then((data) => setRows(extractRows(data)))
      .catch((e) => setError(e instanceof Error ? e.message : 'Could not load'))
      .finally(() => setLoading(false))
  }, [module])

  if (!module) return null

  return (
    <BottomSheet
      ref={sheetRef}
      index={0}
      snapPoints={snapPoints}
      onClose={onClose}
      enablePanDownToClose
      backgroundStyle={styles.sheetBg}
      handleIndicatorStyle={{ backgroundColor: colors.textMuted }}
      backdropComponent={(p) => <BottomSheetBackdrop {...p} disappearsOnIndex={-1} appearsOnIndex={0} />}
    >
      <View style={styles.header}>
        <Text style={styles.headerTitle}>{module.label}</Text>
        {module.summaryOnly && <Text style={styles.headerSub}>Read-only summary — full controls coming later</Text>}
      </View>
      {loading && <ActivityIndicator style={{ marginTop: 20 }} color={colors.accentPurple} />}
      {error && <Text style={styles.error}>{error}</Text>}
      {!loading && !error && rows.length === 0 && <Text style={styles.empty}>Nothing here yet.</Text>}
      <BottomSheetFlatList
        data={rows}
        keyExtractor={(_, i) => String(i)}
        contentContainerStyle={{ paddingHorizontal: 16, paddingBottom: 24 }}
        renderItem={({ item }) => (
          <View style={styles.row}>
            <View style={[styles.dot, { backgroundColor: rowDotColor(item) }]} />
            <View style={{ flex: 1 }}>
              <Text style={styles.rowTitle}>{rowTitle(item)}</Text>
              <Text style={styles.rowMeta} numberOfLines={1}>{rowMeta(item)}</Text>
            </View>
          </View>
        )}
      />
    </BottomSheet>
  )
}

const styles = StyleSheet.create({
  sheetBg: { backgroundColor: colors.cardBg, borderTopLeftRadius: radii.sheetTop, borderTopRightRadius: radii.sheetTop },
  header: { paddingHorizontal: 16, paddingBottom: 12, borderBottomWidth: 1, borderBottomColor: colors.separator },
  headerTitle: { color: colors.textPrimary, fontSize: 16, fontWeight: '700' },
  headerSub: { color: colors.textMuted, fontSize: 11, marginTop: 4 },
  error: { color: colors.accentRed, textAlign: 'center', marginTop: 20 },
  empty: { color: colors.textMuted, textAlign: 'center', marginTop: 20 },
  row: {
    flexDirection: 'row',
    alignItems: 'center',
    gap: 10,
    paddingVertical: 12,
    borderBottomWidth: 1,
    borderBottomColor: colors.separator,
  },
  dot: { width: 8, height: 8, borderRadius: 4 },
  rowTitle: { color: colors.textPrimary, fontSize: 14, fontWeight: '600' },
  rowMeta: { color: colors.textMuted, fontSize: 11, marginTop: 2 },
})
