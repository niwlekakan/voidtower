import React, { useState } from 'react'
import { Platform, SafeAreaView, StyleSheet, Text, TextInput, View } from 'react-native'
import { api } from '../api/client'
import { useAuth } from '../auth/AuthContext'
import { Button } from '../components/Button'
import { Card } from '../components/Card'
import { colors } from '../theme/tokens'

// backend/src/api/node_enroll.rs generates the WireGuard keypair and
// heartbeat token server-side — the device only needs the admin-issued
// pairing code and a display name.
export function NodeEnrollmentScreen() {
  const { logout } = useAuth()
  const [code, setCode] = useState('')
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [enrolled, setEnrolled] = useState<string | null>(null)

  const submit = async () => {
    setBusy(true)
    setError(null)
    try {
      const res = await api.nodes.enroll({
        pairing_code: code.trim(),
        display_name: Platform.OS === 'ios' ? 'iPhone' : 'Android device',
        device_type: 'phone',
        agent_capable: Platform.OS === 'android', // iOS can't run a background node agent
      })
      setEnrolled(res.node_id)
    } catch (e) {
      setError(e instanceof Error ? e.message : 'Enrollment failed')
    } finally {
      setBusy(false)
    }
  }

  return (
    <SafeAreaView style={styles.screen}>
      <Text style={styles.title}>Join the network</Text>
      <Text style={styles.subtitle}>Enter the pairing code shown in VoidTower's admin settings.</Text>
      <Card style={{ gap: 4 }}>
        {enrolled ? (
          <Text style={styles.success}>Enrolled as node {enrolled}.</Text>
        ) : (
          <>
            <TextInput
              style={styles.input}
              value={code}
              onChangeText={setCode}
              placeholder="Pairing code"
              placeholderTextColor={colors.textMuted}
              autoCapitalize="characters"
              autoCorrect={false}
            />
            {error && <Text style={styles.error}>{error}</Text>}
            <View style={{ height: 12 }} />
            <Button title="Enroll this device" onPress={submit} loading={busy} disabled={!code.trim()} />
          </>
        )}
      </Card>
      <View style={{ height: 16 }} />
      <Button title="Log out" variant="secondary" onPress={logout} />
    </SafeAreaView>
  )
}

const styles = StyleSheet.create({
  screen: { flex: 1, backgroundColor: colors.screenBg, padding: 20, justifyContent: 'center' },
  title: { color: colors.textPrimary, fontSize: 20, fontWeight: '700', textAlign: 'center' },
  subtitle: { color: colors.textSecondary, fontSize: 12, textAlign: 'center', marginTop: 6, marginBottom: 20 },
  input: {
    backgroundColor: 'rgba(255,255,255,0.04)',
    borderWidth: 1,
    borderColor: colors.cardBorder,
    borderRadius: 10,
    paddingHorizontal: 12,
    paddingVertical: 10,
    color: colors.textPrimary,
    fontSize: 15,
  },
  error: { color: colors.accentRed, fontSize: 12, marginTop: 10 },
  success: { color: colors.accentGreen, fontSize: 14, textAlign: 'center' },
})
