import { AnimatedBackground, useThemeStore } from 'voidtower-frontend'

// Single export only: bgPreset is global Zustand state shared by every
// mounted instance — multiple exports in the same page would fight over it.
useThemeStore.setState({
  bgPreset: 'void',
  animConfig: {
    speed: 1.0, opacity: 1.0, colorPrimary: '', colorSecondary: '', colorTertiary: '',
    particleCount: 70, particleSize: 1.0, directionAngle: 0, trailOpacity: 0.10,
    gridCols: 16, hexSize: 36, pulseSourceCount: 3, auroraAmplitude: 0.12, glowIntensity: 12,
  },
})

export function Void() {
  return (
    <div style={{ position: 'relative', width: 640, height: 360, overflow: 'hidden' }}>
      <AnimatedBackground />
    </div>
  )
}
