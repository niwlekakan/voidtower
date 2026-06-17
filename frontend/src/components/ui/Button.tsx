import { clsx } from 'clsx'
import type { ButtonHTMLAttributes } from 'react'

type Variant = 'primary' | 'secondary' | 'danger' | 'ghost'
type Size = 'sm' | 'md' | 'lg'

interface ButtonProps extends ButtonHTMLAttributes<HTMLButtonElement> {
  variant?: Variant
  size?: Size
  loading?: boolean
}

const base = 'vt-btn inline-flex items-center justify-center gap-2 font-medium rounded transition-colors focus-visible:outline-none disabled:opacity-50 disabled:cursor-not-allowed'

const sizes: Record<Size, string> = {
  sm: 'px-2.5 py-1 text-xs',
  md: 'px-3.5 py-1.5 text-sm',
  lg: 'px-5 py-2 text-sm',
}

const variantStyles: Record<Variant, React.CSSProperties> = {
  primary:   { background: 'var(--accent-primary)',   color: '#fff' },
  secondary: { background: 'var(--bg-elevated)',      color: 'var(--text-primary)', border: '1px solid var(--border-default)' },
  danger:    { background: 'var(--accent-danger)',    color: '#fff' },
  ghost:     { background: 'transparent',             color: 'var(--text-secondary)' },
}

export default function Button({
  variant = 'secondary',
  size = 'md',
  loading,
  children,
  className,
  disabled,
  style,
  ...props
}: ButtonProps) {
  return (
    <button
      className={clsx(base, sizes[size], className)}
      style={{ ...variantStyles[variant], ...style }}
      disabled={disabled || loading}
      {...props}
    >
      {loading && (
        <span className="w-3 h-3 border-2 border-current border-t-transparent rounded-full animate-spin" />
      )}
      {children}
    </button>
  )
}
