import React, { useState } from 'react'
import { StyleSheet, Text, TextInput, View } from 'react-native'
import { api } from '../api/client'
import { useAuth } from '../auth/AuthContext'
import { Button } from '../components/Button'
import { Card } from '../components/Card'
import { colors } from '../theme/tokens'

// Mirrors the web frontend's ForcePasswordChange flow/copy
// (frontend/src/components/ui/ForcePasswordChange.tsx) — first-login accounts
// (guests especially) must set a real password before they can do anything else.
export function ForcePasswordChangeScreen() {
  const { refreshMe, logout } = useAuth()
  const [password, setPassword] = useState('')
  const [confirm, setConfirm] = useState('')
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)

  const submit = async () => {
    if (password.length < 8) { setError('Password must be at least 8 characters'); return }
    if (password !== confirm) { setError("Passwords don't match"); return }
    setBusy(true)
    setError(null)
    try {
      await api.users.changePassword(password)
      await refreshMe()
    } catch (e) {
      setError(e instanceof Error ? e.message : 'Could not update password')
    } finally {
      setBusy(false)
    }
  }

  return (
    <View style={styles.screen}>
      <Text style={styles.title}>Set a new password</Text>
      <Text style={styles.subtitle}>This account needs a new password before continuing.</Text>
      <Card style={{ gap: 4 }}>
        <TextInput
          style={styles.input}
          value={password}
          onChangeText={setPassword}
          placeholder="New password"
          placeholderTextColor={colors.textMuted}
          secureTextEntry
        />
        <View style={{ height: 12 }} />
        <TextInput
          style={styles.input}
          value={confirm}
          onChangeText={setConfirm}
          placeholder="Confirm password"
          placeholderTextColor={colors.textMuted}
          secureTextEntry
        />
        {error && <Text style={styles.error}>{error}</Text>}
        <View style={{ height: 16 }} />
        <Button title="Set password" onPress={submit} loading={busy} />
        <View style={{ height: 8 }} />
        <Button title="Log out instead" variant="secondary" onPress={logout} />
      </Card>
    </View>
  )
}

const styles = StyleSheet.create({
  screen: { flex: 1, backgroundColor: colors.screenBg, justifyContent: 'center', padding: 20 },
  title: { color: colors.textPrimary, fontSize: 22, fontWeight: '700', textAlign: 'center' },
  subtitle: { color: colors.textSecondary, fontSize: 13, textAlign: 'center', marginTop: 6, marginBottom: 20 },
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
})
