import { Ionicons } from '@expo/vector-icons'
import { createBottomTabNavigator } from '@react-navigation/bottom-tabs'
import { createNativeStackNavigator } from '@react-navigation/native-stack'
import React from 'react'
import { useAuth } from '../auth/AuthContext'
import { SYSTEM_ACCESS_ROLES } from '../hooks/useSystemStatus'
import { AdvancedScreen } from '../screens/AdvancedScreen'
import { ComingSoonScreen } from '../screens/ComingSoonScreen'
import { HomeScreen } from '../screens/HomeScreen'
import { NodeEnrollmentScreen } from '../screens/NodeEnrollmentScreen'
import { colors } from '../theme/tokens'

const Tab = createBottomTabNavigator()
const SettingsStack = createNativeStackNavigator()

const ICONS: Record<string, keyof typeof Ionicons.glyphMap> = {
  Home: 'home',
  Media: 'tv',
  Devices: 'grid',
  Audio: 'volume-high',
  Advanced: 'hardware-chip',
  Settings: 'settings-sharp',
}

function SettingsStackScreen() {
  return (
    <SettingsStack.Navigator screenOptions={{ headerShown: false }}>
      <SettingsStack.Screen name="NodeEnrollment" component={NodeEnrollmentScreen} />
    </SettingsStack.Navigator>
  )
}

/**
 * Home is the real default landing screen for every tier — plain-language
 * status, not a metrics dashboard (see HomeScreen). Advanced (renamed from
 * Tower) carries the full technical view and is hidden for operator/viewer/
 * guest; demo sees it with seed data (AdvancedScreen handles that via
 * useSystemStatus). Media/Devices/Audio are still the shared "coming soon"
 * screen for every tier — those integrations don't exist yet regardless of
 * role.
 */
export function TabNavigator() {
  const { user } = useAuth()
  const showAdvanced = !!user && (SYSTEM_ACCESS_ROLES as readonly string[]).includes(user.role)

  return (
    <Tab.Navigator
      screenOptions={({ route }) => ({
        headerShown: false,
        tabBarActiveTintColor: colors.accentPurple,
        tabBarInactiveTintColor: colors.textMuted,
        tabBarStyle: { backgroundColor: colors.screenBg, borderTopColor: colors.separator },
        tabBarLabelStyle: { fontSize: 8, fontWeight: '600', letterSpacing: 0.5 },
        tabBarIcon: ({ color, size }) => <Ionicons name={ICONS[route.name]} color={color} size={size} />,
      })}
    >
      <Tab.Screen name="Home" component={HomeScreen} />
      <Tab.Screen name="Media">{() => <ComingSoonScreen tab="Media" />}</Tab.Screen>
      <Tab.Screen name="Devices">{() => <ComingSoonScreen tab="Devices" />}</Tab.Screen>
      <Tab.Screen name="Audio">{() => <ComingSoonScreen tab="Audio" />}</Tab.Screen>
      {showAdvanced && <Tab.Screen name="Advanced" component={AdvancedScreen} />}
      <Tab.Screen name="Settings" component={SettingsStackScreen} />
    </Tab.Navigator>
  )
}
