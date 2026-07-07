import React, { useState } from 'react'
import { RefreshControl, SafeAreaView, ScrollView, StyleSheet, Text, TouchableOpacity, View } from 'react-native'
import { MODULE_ENDPOINTS } from '../api/client'
import { Card } from '../components/Card'
import { MetricTile } from '../components/MetricTile'
import { ModuleDrawer, ModuleDef } from '../components/ModuleDrawer'
import { SectionLabel } from '../components/SectionLabel'
import { useSystemStatus } from '../hooks/useSystemStatus'
import { colors, severityColors } from '../theme/tokens'

// Plain-language renames for the ones that read as jargon; everything else
// (Firewall, WireGuard, VMs, Secrets, ...) keeps its technical name on
// purpose — this IS the technical/advanced view, just reached deliberately.
const MODULES: ModuleDef[] = [
  { id: 'services', label: 'Services', endpoint: MODULE_ENDPOINTS.services },
  { id: 'containers', label: 'Containers', endpoint: MODULE_ENDPOINTS.containers },
  { id: 'appvault', label: 'App Vault', endpoint: MODULE_ENDPOINTS.appvault },
  { id: 'vms', label: 'VMs', endpoint: MODULE_ENDPOINTS.vms },
  { id: 'files', label: 'Files', endpoint: MODULE_ENDPOINTS.files, summaryOnly: true },
  { id: 'terminal', label: 'Terminal', endpoint: MODULE_ENDPOINTS.terminal, summaryOnly: true },
  { id: 'backups', label: 'Backups', endpoint: MODULE_ENDPOINTS.backups },
  { id: 'network', label: 'Network', endpoint: MODULE_ENDPOINTS.network },
  { id: 'storage', label: 'Storage', endpoint: MODULE_ENDPOINTS.storage },
  { id: 'firewall', label: 'Firewall', endpoint: MODULE_ENDPOINTS.firewall, summaryOnly: true },
  { id: 'wireguard', label: 'WireGuard', endpoint: MODULE_ENDPOINTS.wireguard },
  { id: 'proxies', label: 'Proxies', endpoint: MODULE_ENDPOINTS.proxies },
  { id: 'automation', label: 'Routines', endpoint: MODULE_ENDPOINTS.automation },
  { id: 'secrets', label: 'Secrets', endpoint: MODULE_ENDPOINTS.secrets },
  { id: 'timeline', label: 'Activity Log', endpoint: MODULE_ENDPOINTS.timeline },
  { id: 'models', label: 'AI Models', endpoint: MODULE_ENDPOINTS.models },
  { id: 'diagnostics', label: 'Health Check', endpoint: MODULE_ENDPOINTS.diagnostics, summaryOnly: true },
  { id: 'security', label: 'Security', endpoint: MODULE_ENDPOINTS.security },
  { id: 'updates', label: 'Updates', endpoint: MODULE_ENDPOINTS.updates, summaryOnly: true },
]

export function AdvancedScreen() {
  const { metrics, alerts, isDemo, refresh } = useSystemStatus()
  const [refreshing, setRefreshing] = useState(false)
  const [openModule, setOpenModule] = useState<ModuleDef | null>(null)

  const onRefresh = async () => {
    setRefreshing(true)
    await refresh()
    setRefreshing(false)
  }

  const ramPct = metrics ? (metrics.ram_used / metrics.ram_total) * 100 : 0
  const storagePct = metrics && metrics.disks[0] ? (metrics.disks[0].used / metrics.disks[0].total) * 100 : 0
  const gpu = metrics?.gpu?.[0]

  return (
    <SafeAreaView style={styles.screen}>
      <ScrollView
        contentContainerStyle={styles.content}
        refreshControl={<RefreshControl refreshing={refreshing} onRefresh={onRefresh} tintColor={colors.accentPurple} />}
      >
        <View style={styles.header}>
          <Text style={styles.title}>Advanced</Text>
          <Text style={styles.subtitle}>
            {metrics ? `${metrics.hostname} · ${Math.floor(metrics.uptime_secs / 3600)}h uptime` : 'Loading…'}
            {isDemo ? ' · DEMO' : ''}
          </Text>
        </View>

        <View style={styles.grid2}>
          <MetricTile label="CPU" value={metrics ? `${metrics.cpu_usage.toFixed(0)}%` : '—'} sub={metrics?.cpu_model ?? ''} pct={metrics?.cpu_usage ?? 0} color={colors.accentGreen} />
          <MetricTile label="MEMORY" value={metrics ? `${ramPct.toFixed(0)}%` : '—'} sub={metrics ? `${(metrics.ram_used / 1e9).toFixed(1)} / ${(metrics.ram_total / 1e9).toFixed(1)} GB` : ''} pct={ramPct} color={colors.accentCyan} />
        </View>
        <View style={styles.grid2}>
          <MetricTile label="STORAGE" value={metrics ? `${storagePct.toFixed(0)}%` : '—'} sub={metrics?.disks[0]?.mount_point ?? ''} pct={storagePct} color={colors.accentPurple} />
          <MetricTile label="GPU" value={gpu ? `${gpu.util_pct.toFixed(0)}%` : '—'} sub={gpu ? gpu.name : 'No GPU detected'} pct={gpu?.util_pct ?? 0} color={colors.accentAmber} />
        </View>

        <SectionLabel>Odysseus</SectionLabel>
        <Card style={styles.aiCard}>
          <View style={[styles.dot, { backgroundColor: colors.accentGreen }]} />
          <Text style={styles.aiText}>AI Orchestrator is reachable from this device</Text>
        </Card>

        <SectionLabel>Alerts</SectionLabel>
        {alerts.length === 0 && <Text style={styles.empty}>No active alerts.</Text>}
        {alerts.map((a) => (
          <Card key={a.id} style={[styles.alertCard, { borderLeftColor: severityColors[a.severity], borderLeftWidth: 3 }]}>
            <Text style={styles.alertTitle}>{a.title}</Text>
            <Text style={styles.alertMsg} numberOfLines={2}>{a.message}</Text>
          </Card>
        ))}

        <SectionLabel>Modules</SectionLabel>
        <View style={styles.moduleGrid}>
          {MODULES.map((m) => (
            <TouchableOpacity key={m.id} style={styles.moduleCard} onPress={() => setOpenModule(m)} activeOpacity={0.7}>
              <Text style={styles.moduleLabel}>{m.label}</Text>
              {m.summaryOnly && <Text style={styles.moduleSub}>summary</Text>}
            </TouchableOpacity>
          ))}
        </View>
      </ScrollView>
      <ModuleDrawer module={isDemo ? null : openModule} onClose={() => setOpenModule(null)} />
    </SafeAreaView>
  )
}

const styles = StyleSheet.create({
  screen: { flex: 1, backgroundColor: colors.screenBg },
  content: { padding: 16, paddingBottom: 48, gap: 12 },
  header: { marginBottom: 4 },
  title: { color: colors.textPrimary, fontSize: 22, fontWeight: '700' },
  subtitle: { color: colors.textSecondary, fontSize: 12, marginTop: 2 },
  grid2: { flexDirection: 'row', gap: 11 },
  aiCard: { flexDirection: 'row', alignItems: 'center', gap: 10 },
  dot: { width: 8, height: 8, borderRadius: 4 },
  aiText: { color: colors.textPrimary, fontSize: 13, flex: 1 },
  empty: { color: colors.textMuted, fontSize: 12 },
  alertCard: { gap: 4 },
  alertTitle: { color: colors.textPrimary, fontSize: 14, fontWeight: '700' },
  alertMsg: { color: colors.textSecondary, fontSize: 12 },
  moduleGrid: { flexDirection: 'row', flexWrap: 'wrap', gap: 10 },
  moduleCard: {
    width: '31%',
    backgroundColor: colors.cardBg,
    borderColor: colors.cardBorder,
    borderWidth: 1,
    borderRadius: 14,
    padding: 12,
    gap: 4,
  },
  moduleLabel: { color: colors.textPrimary, fontSize: 12, fontWeight: '600' },
  moduleSub: { color: colors.textMuted, fontSize: 9 },
})
