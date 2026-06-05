import { useEffect, useRef } from 'react'
import { Terminal as XTerm } from '@xterm/xterm'
import { FitAddon } from '@xterm/addon-fit'
import '@xterm/xterm/css/xterm.css'
import { api } from '@/api/client'

interface Props {
  height?: number
  initialCommand?: string  // pasted into the terminal on connect (no auto-enter)
}

export default function MiniTerminal({ height = 220, initialCommand }: Props) {
  const containerRef = useRef<HTMLDivElement>(null)
  const xtermRef = useRef<XTerm | null>(null)
  const wsRef = useRef<WebSocket | null>(null)

  useEffect(() => {
    const container = containerRef.current
    if (!container || xtermRef.current) return

    const term = new XTerm({
      fontFamily: 'JetBrains Mono, Fira Code, Cascadia Code, SF Mono, Consolas, monospace',
      fontSize: 12,
      theme: { background: '#020403', foreground: '#d7ffe8', cursor: '#8b5cf6' },
      cursorBlink: true,
      scrollback: 1000,
    })
    const fit = new FitAddon()
    term.loadAddon(fit)
    term.open(container)
    fit.fit()
    xtermRef.current = term

    const ws = new WebSocket(api.terminal.wsUrl())
    wsRef.current = ws

    ws.onopen = () => {
      ws.send(JSON.stringify({ type: 'Resize', cols: term.cols, rows: term.rows }))
      // Paste the initial command after a short delay so the shell prompt is ready
      if (initialCommand) {
        setTimeout(() => {
          if (ws.readyState === WebSocket.OPEN)
            ws.send(JSON.stringify({ type: 'Input', data: initialCommand }))
        }, 600)
      }
    }
    ws.onmessage = (e) => {
      try {
        const msg = JSON.parse(e.data)
        if (msg.type === 'Output') term.write(msg.data)
        if (msg.type === 'Closed') term.writeln('\r\n\x1b[31m[session closed]\x1b[0m')
      } catch { /* ignore parse errors */ }
    }
    ws.onclose = () => term.writeln('\r\n\x1b[33m[disconnected]\x1b[0m')
    ws.onerror = () => term.writeln('\r\n\x1b[31m[connection error]\x1b[0m')
    term.onData((data) => {
      if (ws.readyState === WebSocket.OPEN) ws.send(JSON.stringify({ type: 'Input', data }))
    })

    const ro = new ResizeObserver(() => {
      fit.fit()
      if (ws.readyState === WebSocket.OPEN)
        ws.send(JSON.stringify({ type: 'Resize', cols: term.cols, rows: term.rows }))
    })
    ro.observe(container)

    return () => {
      ro.disconnect()
      ws.close()
      term.dispose()
      xtermRef.current = null
      wsRef.current = null
    }
  }, []) // intentionally empty deps — runs once on mount

  return (
    <div
      ref={containerRef}
      style={{ height, background: '#020403', borderRadius: 6, overflow: 'hidden', padding: '4px 2px' }}
    />
  )
}
