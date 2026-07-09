import { useNavigation } from '@react-navigation/native'
import React from 'react'
import { SafeAreaView, ScrollView, StyleSheet, Text, TouchableOpacity, View } from 'react-native'
import { useAuth } from '../auth/AuthContext'
import { Card } from '../components/Card'
import { useSystemStatus } from '../hooks/useSystemStatus'
import { colors, severityColors } from '../theme/tokens'

function greeting() {
  const hour = new Date().getHours()
  if (hour < 12) return 'Good morning'
  if (hour < 18) return 'Good afternoon'
  return 'Good evening'
}

function today() {
  return new Date().toLocaleDateString(undefined, { weekday: 'long', month: 'long', day: 'numeric' })
}

/**
 * The real default landing screen for every account tier — plain-language
 * status, not a metrics dashboard. Owner/admin/demo get a one-line system
 * summary (tap through to Advanced for the technical view); everyone else
 * just gets a friendly welcome, same as before this restructure.
 */
export function HomeScreen() {
  const { user } = useAuth()
  const navigation = useNavigation<any>()
  const { alerts, hasSystemAccess, isDemo } = useSystemStatus()

  const activeAlerts = alerts.filter((a) => a.state === 'active')
  const worstSeverity = activeAlerts.reduce<'info' | 'warning' | 'critical' | null>((worst, a) => {
    const rank = { info: 0, warning: 1, critical: 2 }
    if (!worst || rank[a.severity] > rank[worst]) return a.severity
    return worst
  }, null)

  return (
    <SafeAreaView style={styles.screen}>
      <ScrollView contentContainerStyle={styles.content}>
        <View style={styles.header}>
          <Text style={styles.greeting}>{greeting()}{user ? `, ${user.username}` : ''}</Text>
          <Text style={styles.date}>{today()}</Text>
        </View>

        {hasSystemAccess ? (
          <TouchableOpacity activeOpacity={0.8} onPress={() => navigation.navigate('Advanced')}>
            <Card style={styles.statusCard}>
              <View
                style={[
                  styles.statusDot,
                  { backgroundColor: worstSeverity ? severityColors[worstSeverity] : colors.accentGreen },
                ]}
              />
              <View style={{ flex: 1 }}>
                <Text style={styles.statusTitle}>
                  {activeAlerts.length === 0
                    ? 'Everything looks good'
                    : `${activeAlerts.length} thing${activeAlerts.length === 1 ? '' : 's'} need attention`}
                </Text>
                <Text style={styles.statusSub}>
                  {isDemo ? 'Demo data · tap for the technical view' : 'Tap for the technical view'}
                </Text>
              </View>
            </Card>
          </TouchableOpacity>
        ) : (
          <Card style={styles.welcomeCard}>
            <Text style={styles.statusTitle}>Welcome to VoidTower</Text>
            <Text style={styles.statusSub}>
              Smart home controls — scenes, rooms, lights, and media — are coming soon.
            </Text>
          </Card>
        )}
      </ScrollView>
    </SafeAreaView>
  )
}

const styles = StyleSheet.create({
  screen: { flex: 1, backgroundColor: colors.screenBg },
  content: { padding: 18, paddingBottom: 48, gap: 14 },
  header: { marginBottom: 4 },
  greeting: { color: colors.textPrimary, fontSize: 22, fontWeight: '700' },
  date: { color: colors.textSecondary, fontSize: 13, marginTop: 2 },
  statusCard: { flexDirection: 'row', alignItems: 'center', gap: 12 },
  statusDot: { width: 12, height: 12, borderRadius: 6 },
  statusTitle: { color: colors.textPrimary, fontSize: 15, fontWeight: '700' },
  statusSub: { color: colors.textSecondary, fontSize: 12, marginTop: 2 },
  welcomeCard: { gap: 4 },
})
