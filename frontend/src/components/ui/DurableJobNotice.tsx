import { Loader2 } from 'lucide-react'
import { Link } from 'react-router-dom'
import type { DurableJobSummary } from '@/api/types'
import { durableJobStateLabel, durableJobTone } from '@/hooks/useDurableJobTracker'

const COLORS = {
  info: 'var(--accent-primary)',
  success: 'var(--accent-success)',
  warning: 'var(--accent-warning, #f59e0b)',
  error: 'var(--accent-danger)',
} as const

export default function DurableJobNotice({
  job,
  label,
  tracking,
}: {
  job: DurableJobSummary | null
  label: string | null
  tracking: boolean
}) {
  if (!job || !label) return null
  const color = COLORS[durableJobTone(job.state)]
  return (
    <div
      className="flex items-center gap-2 rounded px-3 py-2 text-xs"
      style={{ background: `${color}18`, border: `1px solid ${color}44`, color }}
    >
      {tracking && <Loader2 size={12} className="animate-spin" />}
      <span>
        {label}: {durableJobStateLabel(job.state)} · job{' '}
        <Link to={`/jobs/${encodeURIComponent(job.id)}`} className="font-mono underline decoration-dotted underline-offset-2 hover:opacity-80" title={`Open durable job ${job.id}`}>
          {job.id.slice(0, 8)}
        </Link>
      </span>
    </div>
  )
}
