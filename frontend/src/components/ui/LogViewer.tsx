import { useEffect, useRef } from 'react'

interface LogViewerProps {
  lines: string[]
  maxHeight?: number
}

export default function LogViewer({ lines, maxHeight = 400 }: LogViewerProps) {
  const ref = useRef<HTMLDivElement>(null)

  useEffect(() => {
    if (ref.current) ref.current.scrollTop = ref.current.scrollHeight
  }, [lines])

  return (
    <div
      ref={ref}
      className="overflow-auto font-mono text-xs p-3 rounded"
      style={{
        maxHeight,
        background: 'var(--terminal-bg)',
        color: 'var(--terminal-green)',
        border: '1px solid var(--border-subtle)',
      }}
    >
      {lines.map((line, i) => (
        <div key={i} className="whitespace-pre-wrap break-all leading-5">{line}</div>
      ))}
      {!lines.length && <span style={{ color: 'var(--text-muted)' }}>No output.</span>}
    </div>
  )
}
