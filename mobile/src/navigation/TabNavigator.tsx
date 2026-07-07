import { Ionicons } from '@expo/vector-icons'
import { createBottomTabNavigator } from '@react-navigation/bottom-tabs'
import { createNativeStackNavigator } from '@react-navigation/native-stack'
import React from 'react'
import { useAuth } from '../auth/AuthContext'
import { ComingSoonScreen } from '../screens/ComingSoonScreen'
import { NodeEnrollmentScreen } from '../screens/NodeEnrollmentScreen'
import { TowerScreen } from '../screens/TowerScreen'
import { colors } from '../theme/tokens'

const Tab = createBottomTabNavigator()
const SettingsStack = createNativeStackNavigator()

const ICONS: Record<string, keyof typeof Ionicons.glyphMap> = {
  Home: 'home',
  Media: 'tv',
  Devices: 'grid',
  Audio: 'volume-high',
  Tower: 'server',
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
 * Tower is hidden for operator/guest (phase-one household/guest tiers).
 * Demo sees Tower with seed data (TowerScreen handles that internally).
 * Home/Media/Devices/Audio are the shared "coming soon" screen for every
 * tier in phase one — those integrations don't exist yet regardless of role.
 */
export function TabNavigator() {
  const { user } = useAuth()
  const showTower = user && ['owner', 'admin', 'demo'].includes(user.role)

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
      <Tab.Screen name="Home">{() => <ComingSoonScreen tab="Home" />}</Tab.Screen>
      <Tab.Screen name="Media">{() => <ComingSoonScreen tab="Media" />}</Tab.Screen>
      <Tab.Screen name="Devices">{() => <ComingSoonScreen tab="Devices" />}</Tab.Screen>
      <Tab.Screen name="Audio">{() => <ComingSoonScreen tab="Audio" />}</Tab.Screen>
      {showTower && <Tab.Screen name="Tower" component={TowerScreen} />}
      <Tab.Screen name="Settings" component={SettingsStackScreen} />
    </Tab.Navigator>
  )
}
