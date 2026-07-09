import { NavigationContainer } from '@react-navigation/native'
import React from 'react'
import { ActivityIndicator, View } from 'react-native'
import { useAuth } from '../auth/AuthContext'
import { ForcePasswordChangeScreen } from '../screens/ForcePasswordChangeScreen'
import { LoginScreen } from '../screens/LoginScreen'
import { colors } from '../theme/tokens'
import { TabNavigator } from './TabNavigator'

export function RootNavigator() {
  const { loading, user } = useAuth()

  if (loading) {
    return (
      <View style={{ flex: 1, backgroundColor: colors.screenBg, alignItems: 'center', justifyContent: 'center' }}>
        <ActivityIndicator color={colors.accentPurple} size="large" />
      </View>
    )
  }

  return (
    <NavigationContainer theme={{
      dark: true,
      colors: {
        primary: colors.accentPurple,
        background: colors.screenBg,
        card: colors.screenBg,
        text: colors.textPrimary,
        border: colors.separator,
        notification: colors.accentRed,
      },
      fonts: {
        regular: { fontFamily: 'System', fontWeight: '400' },
        medium: { fontFamily: 'System', fontWeight: '500' },
        bold: { fontFamily: 'System', fontWeight: '700' },
        heavy: { fontFamily: 'System', fontWeight: '800' },
      },
    }}>
      {!user ? <LoginScreen /> : user.force_password_change ? <ForcePasswordChangeScreen /> : <TabNavigator />}
    </NavigationContainer>
  )
}
