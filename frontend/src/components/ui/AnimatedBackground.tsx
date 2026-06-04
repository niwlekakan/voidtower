import { useEffect, useRef } from 'react'
import { useThemeStore } from '@/store/theme'
import type { BgPreset, AnimConfig } from '@/theme/themes'

type Ctx = CanvasRenderingContext2D

const TARGET_FPS = 30
const FRAME_MS = 1000 / TARGET_FPS

// Read a CSS variable from the document root
function cv(name: string) {
  return getComputedStyle(document.documentElement).getPropertyValue(name).trim()
}
// Parse --bg-root to its raw hex so we can use it in rgba() trail fades
function bgRootRgb(): string {
  const raw = cv('--bg-root') || '#050509'
  // convert #rrggbb → "r,g,b"
  const m = raw.match(/^#([0-9a-fA-F]{2})([0-9a-fA-F]{2})([0-9a-fA-F]{2})/)
  if (m) return `${parseInt(m[1],16)},${parseInt(m[2],16)},${parseInt(m[3],16)}`
  return '5,5,9'
}

function c1(cfg: AnimConfig) { return cfg.colorPrimary   || cv('--accent-primary')   || '#8b5cf6' }
function c2(cfg: AnimConfig) { return cfg.colorSecondary || cv('--accent-secondary') || '#06b6d4' }
function c3(cfg: AnimConfig) { return cfg.colorTertiary  || cv('--accent-success')   || '#39ff88' }

// Wrap a tick function with fps cap + visibility pause
function makeTicker(tick: (dt: number) => void): { start: () => void; stop: () => void } {
  let raf = 0
  let last = 0
  let paused = false

  const onVisibility = () => { paused = document.hidden }
  document.addEventListener('visibilitychange', onVisibility)

  const loop = (now: number) => {
    raf = requestAnimationFrame(loop)
    if (paused) return
    const dt = now - last
    if (dt < FRAME_MS) return
    last = now - (dt % FRAME_MS)
    tick(dt)
  }

  return {
    start: () => { raf = requestAnimationFrame(loop) },
    stop:  () => {
      cancelAnimationFrame(raf)
      document.removeEventListener('visibilitychange', onVisibility)
    },
  }
}

// Helper: set a glow shadow that respects the cfg.glowIntensity
function setGlow(ctx: Ctx, color: string, intensity: number) {
  ctx.shadowColor = color
  ctx.shadowBlur  = intensity
}
function clearGlow(ctx: Ctx) {
  ctx.shadowBlur = 0
}

// ─── VOID ─────────────────────────────────────────────────────────────────────
// Particles now have a soft radial gradient body + cross-spike for "star" types,
// and faint comet trails via a per-particle gradient line.
function animVoid(canvas: HTMLCanvasElement, ctx: Ctx, cfg: AnimConfig) {
  const accent    = c1(cfg)
  const secondary = c2(cfg)
  const bgRgb     = bgRootRgb()
  const rad = (cfg.directionAngle * Math.PI) / 180
  const baseVx = Math.sin(rad), baseVy = -Math.cos(rad)
  const glowPx = Math.min(cfg.glowIntensity * 1.2, 28)

  type P = {
    x: number; y: number; vx: number; vy: number
    r: number; life: number; max: number
    star: boolean; col: string
  }

  const make = (): P => {
    let sx = Math.random() * canvas.width, sy = Math.random() * canvas.height
    if (Math.abs(baseVy) > Math.abs(baseVx)) sy = baseVy < 0 ? canvas.height + 10 : -10
    else sx = baseVx > 0 ? -10 : canvas.width + 10
    const spd = (0.3 + Math.random() * 0.9) * cfg.speed
    const star = Math.random() < 0.06
    return {
      x: sx, y: sy,
      vx: baseVx * spd + (Math.random() - 0.5) * 0.4,
      vy: baseVy * spd + (Math.random() - 0.5) * 0.4,
      r:  (0.8 + Math.random() * 1.8) * cfg.particleSize,
      life: 0,
      max:  (180 + Math.random() * 380) / cfg.speed,
      star,
      col: star ? accent : (Math.random() < 0.25 ? secondary : accent),
    }
  }

  const pool: P[] = Array.from({ length: cfg.particleCount }, () => {
    const p = make()
    p.life = Math.random() * p.max
    p.x = Math.random() * canvas.width
    p.y = Math.random() * canvas.height
    return p
  })

  const ticker = makeTicker(() => {
    // Trail: paint a semi-transparent bg-root rect, not a hardcoded dark color
    ctx.fillStyle = `rgba(${bgRgb},${cfg.trailOpacity})`
    ctx.fillRect(0, 0, canvas.width, canvas.height)

    if (Math.random() < 0.3 * cfg.speed) pool.push(make())

    for (let i = pool.length - 1; i >= 0; i--) {
      const p = pool[i]
      p.life++; p.x += p.vx; p.y += p.vy
      const t = p.life / p.max
      const baseA = t < 0.08 ? t / 0.08 : t > 0.85 ? (1 - t) / 0.15 : 1
      const a = baseA * (p.star ? 0.9 : 0.6) * cfg.opacity

      if (p.star) {
        // Glowing cross / star shape
        const sr = p.r * 2.2
        ctx.globalAlpha = Math.min(a, 1)
        setGlow(ctx, p.col, glowPx * baseA)
        // Core dot
        const grad = ctx.createRadialGradient(p.x, p.y, 0, p.x, p.y, sr)
        grad.addColorStop(0, p.col)
        grad.addColorStop(1, 'transparent')
        ctx.fillStyle = grad
        ctx.beginPath(); ctx.arc(p.x, p.y, sr, 0, Math.PI * 2); ctx.fill()
        // Cross spikes
        ctx.strokeStyle = p.col
        ctx.lineWidth = 0.7
        const arm = sr * 2.5
        ctx.beginPath()
        ctx.moveTo(p.x - arm, p.y); ctx.lineTo(p.x + arm, p.y)
        ctx.moveTo(p.x, p.y - arm); ctx.lineTo(p.x, p.y + arm)
        ctx.stroke()
        clearGlow(ctx)
      } else {
        // Soft glowing dot
        ctx.globalAlpha = Math.min(a, 1)
        if (glowPx > 2) setGlow(ctx, p.col, glowPx * 0.5 * baseA)
        const grad = ctx.createRadialGradient(p.x, p.y, 0, p.x, p.y, p.r * 1.5)
        grad.addColorStop(0, p.col)
        grad.addColorStop(1, 'transparent')
        ctx.fillStyle = grad
        ctx.beginPath(); ctx.arc(p.x, p.y, p.r * 1.5, 0, Math.PI * 2); ctx.fill()
        clearGlow(ctx)
      }

      if (p.life >= p.max) pool.splice(i, 1)
    }

    while (pool.length < cfg.particleCount) pool.push(make())
    ctx.globalAlpha = 1
  })

  ticker.start()
  return ticker.stop
}

// ─── GRID ─────────────────────────────────────────────────────────────────────
// Lines now fade from faint at horizon to bright at bottom via opacity gradient.
// Beam dots replaced with bright glow orbs that leave a short gradient tail.
function animGrid(canvas: HTMLCanvasElement, ctx: Ctx, cfg: AnimConfig) {
  const accent    = c1(cfg)
  const secondary = c2(cfg)
  const glowPx    = Math.min(cfg.glowIntensity * 1.5, 32)
  const COLS = cfg.gridCols
  let scroll = 0

  type Beam = { col: number; y: number; speed: number; col2: string }
  const beams: Beam[] = Array.from({ length: Math.max(3, Math.floor(COLS / 5)) }, () => ({
    col:   Math.floor(Math.random() * COLS),
    y:     Math.random(),
    speed: (0.003 + Math.random() * 0.005) * cfg.speed,
    col2:  Math.random() < 0.5 ? accent : secondary,
  }))

  const ticker = makeTicker(() => {
    ctx.clearRect(0, 0, canvas.width, canvas.height)
    scroll = (scroll + 0.006 * cfg.speed) % 1
    const W = canvas.width, H = canvas.height
    const vpY = H * 0.45, vpX = W / 2
    const ROWS = 14

    const project = (gx: number, gy: number) => {
      const t = (gy + scroll) % 1
      return {
        x: vpX + (gx / COLS - 0.5) * W * (0.08 + t * 1.9),
        y: vpY + (H - vpY) * (t * t),
        t,
      }
    }

    // Horizontal lines — brighter and thicker near camera
    for (let row = 0; row <= ROWS; row++) {
      const gy = row / ROWS
      const left = project(0, gy), right = project(COLS, gy)
      if (left.t < 0.015) continue
      const a = Math.pow(left.t, 1.3) * 0.7 * cfg.opacity
      ctx.globalAlpha = a
      ctx.strokeStyle = accent
      ctx.lineWidth = 0.5 + left.t * 1.8
      if (glowPx > 3 && left.t > 0.5) {
        setGlow(ctx, accent, glowPx * left.t * 0.4)
      }
      ctx.beginPath(); ctx.moveTo(left.x, left.y); ctx.lineTo(right.x, right.y); ctx.stroke()
      clearGlow(ctx)
    }

    // Vertical lines — fainter, secondary color
    for (let col = 0; col <= COLS; col++) {
      const top = project(col, 0.01), bot = project(col, 0.98)
      ctx.globalAlpha = 0.14 * cfg.opacity
      ctx.strokeStyle = secondary
      ctx.lineWidth = 0.6
      ctx.beginPath(); ctx.moveTo(top.x, top.y); ctx.lineTo(bot.x, bot.y); ctx.stroke()
    }

    // Beam orbs — radial gradient + glow
    beams.forEach(b => {
      b.y = (b.y + b.speed) % 1
      const pt = project(b.col + 0.5, b.y)
      if (pt.t < 0.05) return
      const r = (4 + pt.t * 5) * cfg.particleSize
      ctx.globalAlpha = pt.t * 0.95 * cfg.opacity
      setGlow(ctx, b.col2, glowPx * pt.t)
      const grad = ctx.createRadialGradient(pt.x, pt.y, 0, pt.x, pt.y, r)
      grad.addColorStop(0, '#ffffff')
      grad.addColorStop(0.25, b.col2)
      grad.addColorStop(1, 'transparent')
      ctx.fillStyle = grad
      ctx.beginPath(); ctx.arc(pt.x, pt.y, r, 0, Math.PI * 2); ctx.fill()
      clearGlow(ctx)
    })

    ctx.globalAlpha = 1
  })

  ticker.start()
  return ticker.stop
}

// ─── AURORA ───────────────────────────────────────────────────────────────────
// Each band uses a vertical gradient that is itself animated. Layers are
// rendered twice: a wide soft base + a narrow bright ridge, giving depth.
// Step stays coarser (12px) for perf but smooth enough at half-res.
function animAurora(canvas: HTMLCanvasElement, ctx: Ctx, cfg: AnimConfig) {
  let t = 0
  const colors = [c1(cfg), c2(cfg), c3(cfg)]
  const baseAmp = cfg.auroraAmplitude
  const STEP = 12 // coarse enough for perf, fine enough at 0.5x res

  const layers = [
    { amp: baseAmp,        freq: 1.3, speed: 0.0006 * cfg.speed, offset: 0.0, ci: 0, alpha: 0.28 * cfg.opacity },
    { amp: baseAmp * 0.7,  freq: 2.0, speed: 0.0010 * cfg.speed, offset: 2.1, ci: 1, alpha: 0.20 * cfg.opacity },
    { amp: baseAmp * 0.85, freq: 0.8, speed: 0.0004 * cfg.speed, offset: 4.5, ci: 2, alpha: 0.16 * cfg.opacity },
    { amp: baseAmp * 0.45, freq: 3.1, speed: 0.0014 * cfg.speed, offset: 1.3, ci: 0, alpha: 0.11 * cfg.opacity },
  ]

  const ticker = makeTicker(() => {
    ctx.clearRect(0, 0, canvas.width, canvas.height)
    t++
    const W = canvas.width, H = canvas.height

    layers.forEach(l => {
      const col = colors[l.ci]
      const cy  = H * (0.28 + Math.sin(t * l.speed * 0.5 + l.offset) * 0.18)
      const bandH = H * l.amp * 3.5

      // Build the wavy bottom path
      ctx.beginPath()
      ctx.moveTo(0, H) // bottom-left corner
      ctx.lineTo(0, cy + Math.sin(l.offset) * H * l.amp)
      for (let x = STEP; x <= W; x += STEP) {
        const y = cy + Math.sin(x / W * Math.PI * 2 * l.freq + t * l.speed * 40 + l.offset) * H * l.amp
        ctx.lineTo(x, y)
      }
      ctx.lineTo(W, H)
      ctx.closePath()

      // Wide soft band
      const grad = ctx.createLinearGradient(0, cy - bandH * 0.2, 0, cy + bandH)
      grad.addColorStop(0,   col)
      grad.addColorStop(0.45, col)
      grad.addColorStop(1,   'transparent')
      ctx.globalAlpha = l.alpha * 0.55
      ctx.fillStyle = grad
      ctx.fill()

      // Bright ridge along the wave top — narrow strip
      ctx.beginPath()
      for (let x = 0; x <= W; x += STEP) {
        const y = cy + Math.sin(x / W * Math.PI * 2 * l.freq + t * l.speed * 40 + l.offset) * H * l.amp
        x === 0 ? ctx.moveTo(x, y) : ctx.lineTo(x, y)
      }
      ctx.globalAlpha = l.alpha * 1.6
      ctx.strokeStyle = col
      ctx.lineWidth = 1.5 + baseAmp * 8
      if (cfg.glowIntensity > 3) setGlow(ctx, col, cfg.glowIntensity * 0.6)
      ctx.stroke()
      clearGlow(ctx)
    })

    ctx.globalAlpha = 1
  })

  ticker.start()
  return ticker.stop
}

// ─── PULSE ────────────────────────────────────────────────────────────────────
// Rings are now rendered with a radial gradient stroke (thick→thin) and a
// double-glow halo. Source orbs breathe with an inner + outer gradient.
function animPulse(canvas: HTMLCanvasElement, ctx: Ctx, cfg: AnimConfig) {
  const accent    = c1(cfg)
  const secondary = c2(cfg)
  const bgRgb     = bgRootRgb()
  const glowPx    = Math.min(cfg.glowIntensity * 1.8, 40)

  type Ring = { x: number; y: number; r: number; max: number; col: string }
  const rings: Ring[] = []
  let frame = 0

  const sources = Array.from({ length: cfg.pulseSourceCount }, () => ({
    x:        0.1 + Math.random() * 0.8,
    y:        0.1 + Math.random() * 0.8,
    interval: Math.floor((70 + Math.random() * 60) / cfg.speed),
    col:      Math.random() < 0.6 ? accent : secondary,
  }))

  const ticker = makeTicker(() => {
    ctx.fillStyle = `rgba(${bgRgb},${cfg.trailOpacity})`
    ctx.fillRect(0, 0, canvas.width, canvas.height)
    const W = canvas.width, H = canvas.height
    frame++

    sources.forEach(s => {
      if (frame % s.interval === 0)
        rings.push({ x: s.x * W, y: s.y * H, r: 0, max: Math.max(W, H) * 0.72, col: s.col })
    })

    for (let i = rings.length - 1; i >= 0; i--) {
      const ring = rings[i]
      ring.r += 2.4 * cfg.speed
      const progress = ring.r / ring.max
      const a = Math.pow(1 - progress, 1.8) * 0.75 * cfg.opacity
      if (a < 0.004) { rings.splice(i, 1); continue }

      // Outer glow ring
      ctx.globalAlpha = a * 0.35
      setGlow(ctx, ring.col, glowPx * (1 - progress))
      ctx.strokeStyle = ring.col
      ctx.lineWidth = 3 + (1 - progress) * 4
      ctx.beginPath(); ctx.arc(ring.x, ring.y, ring.r, 0, Math.PI * 2); ctx.stroke()
      clearGlow(ctx)

      // Inner crisp ring
      ctx.globalAlpha = a * 0.9
      ctx.strokeStyle = ring.col
      ctx.lineWidth = 0.8
      ctx.beginPath(); ctx.arc(ring.x, ring.y, ring.r, 0, Math.PI * 2); ctx.stroke()
    }

    // Source orbs
    sources.forEach(s => {
      const pulse  = (Math.sin(frame * 0.04 * cfg.speed + s.interval) + 1) / 2
      const r      = (4 + pulse * 5) * cfg.particleSize
      const x = s.x * W, y = s.y * H

      // Outer halo
      ctx.globalAlpha = (0.2 + pulse * 0.25) * cfg.opacity
      setGlow(ctx, s.col, glowPx * 0.9)
      const outer = ctx.createRadialGradient(x, y, 0, x, y, r * 3.5)
      outer.addColorStop(0,   s.col)
      outer.addColorStop(0.5, s.col)
      outer.addColorStop(1,  'transparent')
      ctx.fillStyle = outer
      ctx.beginPath(); ctx.arc(x, y, r * 3.5, 0, Math.PI * 2); ctx.fill()
      clearGlow(ctx)

      // Core bright spot
      ctx.globalAlpha = (0.7 + pulse * 0.3) * cfg.opacity
      const inner = ctx.createRadialGradient(x, y, 0, x, y, r)
      inner.addColorStop(0, '#ffffff')
      inner.addColorStop(0.4, s.col)
      inner.addColorStop(1, 'transparent')
      ctx.fillStyle = inner
      ctx.beginPath(); ctx.arc(x, y, r, 0, Math.PI * 2); ctx.fill()
    })

    ctx.globalAlpha = 1
  })

  ticker.start()
  return ticker.stop
}

// ─── NOISE ────────────────────────────────────────────────────────────────────
// Particles are now soft radial-gradient dots instead of square pixels.
// Flow field unchanged (same perf). Adds very subtle glow to busier regions.
function animNoise(canvas: HTMLCanvasElement, ctx: Ctx, cfg: AnimConfig) {
  const accent    = c1(cfg)
  const secondary = c2(cfg)
  const bgRgb     = bgRootRgb()
  const N         = cfg.particleCount
  const dirBias   = (cfg.directionAngle * Math.PI) / 180
  const glowPx    = Math.min(cfg.glowIntensity * 0.6, 12)

  type P = { x: number; y: number; life: number; max: number; col: string }
  const rand = (): P => ({
    x: Math.random() * canvas.width,
    y: Math.random() * canvas.height,
    life: 0,
    max: (180 + Math.random() * 260) / cfg.speed,
    col: Math.random() < 0.25 ? secondary : accent,
  })
  const particles: P[] = Array.from({ length: N }, rand)
  let t = 0

  const field = (x: number, y: number) =>
    Math.sin(x / canvas.width * 4 + t * 0.7) *
    Math.cos(y / canvas.height * 3 + t * 0.5) *
    Math.PI * 2 + dirBias * 0.3

  const ticker = makeTicker(() => {
    ctx.fillStyle = `rgba(${bgRgb},${cfg.trailOpacity})`
    ctx.fillRect(0, 0, canvas.width, canvas.height)
    t += 0.008 * cfg.speed
    const spd = 1.2 * cfg.speed
    const r   = 1.6 * cfg.particleSize

    particles.forEach(p => {
      p.life++
      const angle = field(p.x, p.y)
      p.x += Math.cos(angle) * spd
      p.y += Math.sin(angle) * spd
      if (p.x < 0) p.x += canvas.width
      if (p.x > canvas.width) p.x -= canvas.width
      if (p.y < 0) p.y += canvas.height
      if (p.y > canvas.height) p.y -= canvas.height

      const lt  = p.life / p.max
      const baseA = lt < 0.1 ? lt / 0.1 : lt > 0.8 ? (1 - lt) / 0.2 : 1
      const a   = baseA * 0.65 * cfg.opacity

      ctx.globalAlpha = a
      if (glowPx > 2 && lt > 0.2 && lt < 0.8) setGlow(ctx, p.col, glowPx * baseA)
      const grad = ctx.createRadialGradient(p.x, p.y, 0, p.x, p.y, r * 1.8)
      grad.addColorStop(0, p.col)
      grad.addColorStop(1, 'transparent')
      ctx.fillStyle = grad
      ctx.beginPath(); ctx.arc(p.x, p.y, r * 1.8, 0, Math.PI * 2); ctx.fill()
      clearGlow(ctx)

      if (p.life >= p.max) Object.assign(p, rand())
    })

    ctx.globalAlpha = 1
  })

  ticker.start()
  return ticker.stop
}

// ─── HEX ──────────────────────────────────────────────────────────────────────
// Cells now have a filled gradient interior + glowing edge. A wave propagates
// through neighboring cells for a ripple/shimmer effect.
function animHex(canvas: HTMLCanvasElement, ctx: Ctx, cfg: AnimConfig) {
  const accent    = c1(cfg)
  const secondary = c2(cfg)
  const SIZE   = cfg.hexSize
  const SQRT3  = Math.sqrt(3)
  const glowPx = Math.min(cfg.glowIntensity * 1.4, 36)

  type Hex = { cx: number; cy: number; phase: number; col: string; row: number; colIdx: number }
  const cells: Hex[] = []

  const cols = Math.ceil(canvas.width  / (SIZE * SQRT3)) + 2
  const rows = Math.ceil(canvas.height / (SIZE * 1.5))   + 2

  for (let r = -1; r < rows; r++) {
    for (let c = -1; c < cols; c++) {
      cells.push({
        cx:     c * SIZE * SQRT3 + (r % 2 === 0 ? 0 : SIZE * SQRT3 / 2),
        cy:     r * SIZE * 1.5,
        phase:  Math.random() * Math.PI * 2,
        col:    Math.random() < 0.3 ? secondary : accent,
        row:    r,
        colIdx: c,
      })
    }
  }

  const drawHexPath = (cx: number, cy: number, r: number) => {
    ctx.beginPath()
    for (let i = 0; i < 6; i++) {
      const a = Math.PI / 180 * (60 * i - 30)
      const px = cx + r * Math.cos(a)
      const py = cy + r * Math.sin(a)
      i === 0 ? ctx.moveTo(px, py) : ctx.lineTo(px, py)
    }
    ctx.closePath()
  }

  let t = 0
  const ticker = makeTicker(() => {
    ctx.clearRect(0, 0, canvas.width, canvas.height)
    t++

    cells.forEach(cell => {
      // Wave travels diagonally across the grid
      const wave = Math.sin(t * 0.022 * cfg.speed - cell.row * 0.28 - cell.colIdx * 0.18 + cell.phase * 0.15)
      const pulse = (wave + 1) / 2  // 0..1

      // Interior fill — subtle gradient from center
      const fillA = (0.03 + pulse * 0.12) * cfg.opacity
      ctx.globalAlpha = fillA
      const grad = ctx.createRadialGradient(cell.cx, cell.cy, 0, cell.cx, cell.cy, SIZE * 0.8)
      grad.addColorStop(0,   cell.col)
      grad.addColorStop(0.6, cell.col)
      grad.addColorStop(1,  'transparent')
      ctx.fillStyle = grad
      drawHexPath(cell.cx, cell.cy, SIZE - 2)
      ctx.fill()

      // Edge stroke — brighter during wave peak
      const edgeA = (0.08 + pulse * 0.5) * cfg.opacity
      ctx.globalAlpha = edgeA
      if (glowPx > 2 && pulse > 0.6) setGlow(ctx, cell.col, glowPx * (pulse - 0.6) * 2.5)
      ctx.strokeStyle = cell.col
      ctx.lineWidth   = 0.6 + pulse * 1.4
      drawHexPath(cell.cx, cell.cy, SIZE - 1)
      ctx.stroke()
      clearGlow(ctx)
    })

    ctx.globalAlpha = 1
  })

  ticker.start()
  return ticker.stop
}

// ─── HEX CLASSIC ──────────────────────────────────────────────────────────────
// Original hex: each cell pulses independently via its own random phase.
// Stroke-only flat lines — no fill, no directional wave, no glow passes.
function animHexClassic(canvas: HTMLCanvasElement, ctx: Ctx, cfg: AnimConfig) {
  const accent    = c1(cfg)
  const secondary = c2(cfg)
  const SIZE   = cfg.hexSize
  const SQRT3  = Math.sqrt(3)

  type Hex = { cx: number; cy: number; phase: number; col: string }
  const cells: Hex[] = []

  const cols = Math.ceil(canvas.width  / (SIZE * SQRT3)) + 2
  const rows = Math.ceil(canvas.height / (SIZE * 1.5))   + 2

  for (let r = -1; r < rows; r++) {
    for (let c = -1; c < cols; c++) {
      cells.push({
        cx:    c * SIZE * SQRT3 + (r % 2 === 0 ? 0 : SIZE * SQRT3 / 2),
        cy:    r * SIZE * 1.5,
        phase: Math.random() * Math.PI * 2,
        col:   Math.random() < 0.3 ? secondary : accent,
      })
    }
  }

  const drawHexPath = (cx: number, cy: number, r: number) => {
    ctx.beginPath()
    for (let i = 0; i < 6; i++) {
      const a = Math.PI / 180 * (60 * i - 30)
      i === 0
        ? ctx.moveTo(cx + r * Math.cos(a), cy + r * Math.sin(a))
        : ctx.lineTo(cx + r * Math.cos(a), cy + r * Math.sin(a))
    }
    ctx.closePath()
  }

  let t = 0
  const ticker = makeTicker(() => {
    ctx.clearRect(0, 0, canvas.width, canvas.height)
    t++

    cells.forEach(cell => {
      const pulse = (Math.sin(t * 0.018 * cfg.speed + cell.phase) + 1) / 2
      ctx.globalAlpha = (0.06 + pulse * 0.55) * cfg.opacity
      ctx.strokeStyle = cell.col
      ctx.lineWidth   = 0.5 + pulse * 0.8
      drawHexPath(cell.cx, cell.cy, SIZE - 1)
      ctx.stroke()
    })

    ctx.globalAlpha = 1
  })

  ticker.start()
  return ticker.stop
}

// ─── CIRCUIT ──────────────────────────────────────────────────────────────────
// Traces are now neon-glowing lines (double-pass: glow + crisp). Packets have
// a radial gradient body with a bright white core and a gradient tail behind them.
function animCircuit(canvas: HTMLCanvasElement, ctx: Ctx, cfg: AnimConfig) {
  const accent    = c1(cfg)
  const secondary = c2(cfg)
  const bgRgb     = bgRootRgb()
  const W = canvas.width, H = canvas.height
  const GRID   = Math.max(24, cfg.hexSize)
  const glowPx = Math.min(cfg.glowIntensity * 1.6, 38)

  type Node   = { x: number; y: number }
  type Trace  = { pts: Node[]; length: number; col: string }

  const traces: Trace[] = []
  const usedEdges = new Set<string>()
  const dirs = [[1,0],[-1,0],[0,1],[0,-1]]

  for (let attempt = 0; attempt < 50; attempt++) {
    const pts: Node[] = [{
      x: Math.floor(Math.random() * (W / GRID)) * GRID,
      y: Math.floor(Math.random() * (H / GRID)) * GRID,
    }]
    const steps = 4 + Math.floor(Math.random() * 7)
    for (let s = 0; s < steps; s++) {
      const last = pts[pts.length - 1]
      const [dx, dy] = dirs[Math.floor(Math.random() * 4)]
      const next = { x: last.x + dx * GRID, y: last.y + dy * GRID }
      if (next.x < 0 || next.x > W || next.y < 0 || next.y > H) break
      const key = `${last.x},${last.y}-${next.x},${next.y}`
      if (usedEdges.has(key)) break
      usedEdges.add(key)
      usedEdges.add(`${next.x},${next.y}-${last.x},${last.y}`)
      pts.push(next)
    }
    if (pts.length > 2) {
      traces.push({ pts, length: pts.length - 1, col: Math.random() < 0.6 ? accent : secondary })
    }
  }

  type Packet = { traceIdx: number; pos: number; speed: number; col: string }
  const packets: Packet[] = traces.map((tr, i) => ({
    traceIdx: i,
    pos:      Math.random(),
    speed:    (0.003 + Math.random() * 0.005) * cfg.speed,
    col:      tr.col,
  }))

  const ticker = makeTicker(() => {
    ctx.fillStyle = `rgba(${bgRgb},${cfg.trailOpacity})`
    ctx.fillRect(0, 0, canvas.width, canvas.height)

    // Draw traces — glow pass then crisp pass
    traces.forEach(tr => {
      ctx.beginPath()
      ctx.moveTo(tr.pts[0].x, tr.pts[0].y)
      tr.pts.slice(1).forEach(p => ctx.lineTo(p.x, p.y))

      // Glow pass
      ctx.globalAlpha = 0.12 * cfg.opacity
      setGlow(ctx, tr.col, glowPx * 0.5)
      ctx.strokeStyle = tr.col
      ctx.lineWidth   = 3
      ctx.stroke()
      clearGlow(ctx)

      // Crisp hairline
      ctx.globalAlpha = 0.25 * cfg.opacity
      ctx.strokeStyle = tr.col
      ctx.lineWidth   = 0.8
      ctx.stroke()

      // Junction nodes
      tr.pts.forEach(p => {
        ctx.globalAlpha = 0.4 * cfg.opacity
        const nr = 2.2 * cfg.particleSize
        const ng = ctx.createRadialGradient(p.x, p.y, 0, p.x, p.y, nr)
        ng.addColorStop(0, '#ffffff')
        ng.addColorStop(0.4, tr.col)
        ng.addColorStop(1,  'transparent')
        ctx.fillStyle = ng
        ctx.beginPath(); ctx.arc(p.x, p.y, nr, 0, Math.PI * 2); ctx.fill()
      })
    })

    // Draw packets — gradient body + glow
    packets.forEach(pkt => {
      pkt.pos = (pkt.pos + pkt.speed) % 1
      const tr  = traces[pkt.traceIdx]
      const seg = pkt.pos * tr.length
      const si  = Math.floor(seg)
      const sf  = seg - si
      if (si >= tr.pts.length - 1) return

      const a = tr.pts[si], b = tr.pts[si + 1]
      const x = a.x + (b.x - a.x) * sf
      const y = a.y + (b.y - a.y) * sf
      const r = 5 * cfg.particleSize

      // Halo
      ctx.globalAlpha = 0.3 * cfg.opacity
      setGlow(ctx, pkt.col, glowPx)
      const halo = ctx.createRadialGradient(x, y, 0, x, y, r * 2.8)
      halo.addColorStop(0,   pkt.col)
      halo.addColorStop(0.5, pkt.col)
      halo.addColorStop(1,  'transparent')
      ctx.fillStyle = halo
      ctx.beginPath(); ctx.arc(x, y, r * 2.8, 0, Math.PI * 2); ctx.fill()
      clearGlow(ctx)

      // Core
      ctx.globalAlpha = cfg.opacity
      const core = ctx.createRadialGradient(x, y, 0, x, y, r)
      core.addColorStop(0,   '#ffffff')
      core.addColorStop(0.3, pkt.col)
      core.addColorStop(1,  'transparent')
      ctx.fillStyle = core
      ctx.beginPath(); ctx.arc(x, y, r, 0, Math.PI * 2); ctx.fill()
    })

    ctx.globalAlpha = 1
  })

  ticker.start()
  return ticker.stop
}

// ─── Dispatcher ───────────────────────────────────────────────────────────────
const ANIMS: Record<Exclude<BgPreset, 'none'>, (c: HTMLCanvasElement, ctx: Ctx, cfg: AnimConfig) => () => void> = {
  void: animVoid, grid: animGrid, aurora: animAurora,
  pulse: animPulse, noise: animNoise, hex: animHex,
  'hex-classic': animHexClassic, circuit: animCircuit,
}

export default function AnimatedBackground() {
  const bgPreset   = useThemeStore((s) => s.bgPreset)
  const glassLevel = useThemeStore((s) => s.glassLevel)
  const animConfig = useThemeStore((s) => s.animConfig)
  const canvasRef  = useRef<HTMLCanvasElement>(null)

  useEffect(() => {
    if (glassLevel === 'none') document.body.removeAttribute('data-glass')
    else document.body.setAttribute('data-glass', glassLevel)
  }, [glassLevel])

  useEffect(() => {
    const canvas = canvasRef.current
    if (!canvas || bgPreset === 'none') return
    const ctx = canvas.getContext('2d', { alpha: false })
    if (!ctx) return

    const resize = () => {
      // Render at device pixel ratio (capped at 1.5) so the canvas matches
      // physical pixels on 1× displays and stays sharp on HiDPI without
      // blowing up the pixel budget on 4K/3× screens.
      const dpr = Math.min(window.devicePixelRatio || 1, 1.5)
      canvas.width  = Math.floor(window.innerWidth  * dpr)
      canvas.height = Math.floor(window.innerHeight * dpr)
    }
    resize()
    window.addEventListener('resize', resize)
    const cleanup = ANIMS[bgPreset](canvas, ctx, animConfig)
    return () => { cleanup(); window.removeEventListener('resize', resize) }
  }, [bgPreset, animConfig])

  return (
    <div style={{
      position: 'fixed', inset: 0, zIndex: -1,
      pointerEvents: 'none', overflow: 'hidden',
      background: 'var(--bg-root)',
    }}>
      {bgPreset !== 'none' && (
        <canvas
          ref={canvasRef}
          style={{
            position: 'absolute', inset: 0,
            width: '100%', height: '100%',
            imageRendering: 'auto',
          }}
        />
      )}
    </div>
  )
}
