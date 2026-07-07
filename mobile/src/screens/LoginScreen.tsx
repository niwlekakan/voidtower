import React, { useState } from 'react'
import { KeyboardAvoidingView, Platform, StyleSheet, Text, TextInput, View } from 'react-native'
import { useAuth } from '../auth/AuthContext'
import { Button } from '../components/Button'
import { Card } from '../components/Card'
import { colors } from '../theme/tokens'

/**
 * Two-step flow: point the app at a self-hosted VoidTower server, then log
 * in against it. Server URL is a prerequisite because — unlike a SaaS app —
 * there's no fixed backend host to hardcode.
 */
export function LoginScreen() {
  const { serverUrl, setServer, login, error, totpRequired } = useAuth()
  const [urlInput, setUrlInput] = useState('https://')
  const [username, setUsername] = useState('')
  const [password, setPassword] = useState('')
  const [totpCode, setTotpCode] = useState('')
  const [busy, setBusy] = useState(false)

  if (!serverUrl) {
    return (
      <KeyboardAvoidingView style={styles.screen} behavior={Platform.OS === 'ios' ? 'padding' : undefined}>
        <Text style={styles.title}>VoidTower</Text>
        <Text style={styles.subtitle}>Connect to your server</Text>
        <Card style={styles.card}>
          <Text style={styles.label}>Server URL</Text>
          <TextInput
            style={styles.input}
            value={urlInput}
            onChangeText={setUrlInput}
            autoCapitalize="none"
            autoCorrect={false}
            keyboardType="url"
            placeholder="https://voidtower.example.com"
            placeholderTextColor={colors.textMuted}
          />
          <View style={{ height: 12 }} />
          <Button title="Continue" onPress={() => setServer(urlInput)} disabled={urlInput.trim().length < 8} />
        </Card>
      </KeyboardAvoidingView>
    )
  }

  const handleLogin = async () => {
    setBusy(true)
    try {
      await login(username, password, totpCode || undefined)
    } finally {
      setBusy(false)
    }
  }

  return (
    <KeyboardAvoidingView style={styles.screen} behavior={Platform.OS === 'ios' ? 'padding' : undefined}>
      <Text style={styles.title}>VoidTower</Text>
      <Text style={styles.subtitle}>{serverUrl}</Text>
      <Card style={styles.card}>
        <Text style={styles.label}>Username</Text>
        <TextInput
          style={styles.input}
          value={username}
          onChangeText={setUsername}
          autoCapitalize="none"
          autoCorrect={false}
        />
        <View style={{ height: 12 }} />
        <Text style={styles.label}>Password</Text>
        <TextInput
          style={styles.input}
          value={password}
          onChangeText={setPassword}
          secureTextEntry
          autoCapitalize="none"
        />
        {totpRequired && (
          <>
            <View style={{ height: 12 }} />
            <Text style={styles.label}>2FA code</Text>
            <TextInput
              style={styles.input}
              value={totpCode}
              onChangeText={setTotpCode}
              keyboardType="number-pad"
            />
          </>
        )}
        {error && <Text style={styles.error}>{error}</Text>}
        <View style={{ height: 16 }} />
        <Button title={totpRequired ? 'Verify' : 'Log in'} onPress={handleLogin} loading={busy} disabled={!username || !password} />
        <View style={{ height: 8 }} />
        <Button title="Use a different server" variant="secondary" onPress={() => setServer('')} />
      </Card>
    </KeyboardAvoidingView>
  )
}

const styles = StyleSheet.create({
  screen: { flex: 1, backgroundColor: colors.screenBg, justifyContent: 'center', padding: 20 },
  title: { color: colors.textPrimary, fontSize: 28, fontWeight: '700', textAlign: 'center' },
  subtitle: { color: colors.textSecondary, fontSize: 13, textAlign: 'center', marginTop: 4, marginBottom: 24 },
  card: { gap: 4 },
  label: { color: colors.textSecondary, fontSize: 12, marginBottom: 6 },
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
