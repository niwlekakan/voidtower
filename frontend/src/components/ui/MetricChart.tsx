import { AreaChart, Area, ResponsiveContainer, Tooltip, XAxis } from 'recharts'

interface DataPoint {
  t: number
  v: number
}

interface MetricChartProps {
  data: DataPoint[]
  color?: string
  unit?: string
  height?: number
}

export default function MetricChart({
  data,
  color = 'var(--accent-primary)',
  unit = '',
  height = 80,
}: MetricChartProps) {
  return (
    <ResponsiveContainer width="100%" height={height}>
      <AreaChart data={data} margin={{ top: 4, right: 0, bottom: 0, left: 0 }}>
        <defs>
          <linearGradient id={`grad-${color}`} x1="0" y1="0" x2="0" y2="1">
            <stop offset="5%" stopColor={color} stopOpacity={0.25} />
            <stop offset="95%" stopColor={color} stopOpacity={0} />
          </linearGradient>
        </defs>
        <XAxis dataKey="t" hide />
        <Tooltip
          contentStyle={{ background: 'var(--bg-elevated)', border: '1px solid var(--border-subtle)', borderRadius: 4, fontSize: 12 }}
          labelStyle={{ display: 'none' }}
          formatter={(v: number) => [`${v.toFixed(1)}${unit}`, '']}
        />
        <Area
          type="monotone"
          dataKey="v"
          stroke={color}
          strokeWidth={1.5}
          fill={`url(#grad-${color})`}
          dot={false}
          isAnimationActive={false}
        />
      </AreaChart>
    </ResponsiveContainer>
  )
}
