/** @type {import('tailwindcss').Config} */
export default {
  content: ['./index.html', './src/**/*.{ts,tsx}'],
  theme: {
    extend: {
      colors: {
        'bg-root':      'var(--bg-root)',
        'bg-panel':     'var(--bg-panel)',
        'bg-card':      'var(--bg-card)',
        'bg-elevated':  'var(--bg-elevated)',
        'border-subtle':'var(--border-subtle)',
        'border-default':'var(--border-default)',
        'text-primary': 'var(--text-primary)',
        'text-secondary':'var(--text-secondary)',
        'text-muted':   'var(--text-muted)',
        'accent-primary':'var(--accent-primary)',
        'accent-secondary':'var(--accent-secondary)',
        'accent-success':'var(--accent-success)',
        'accent-warning':'var(--accent-warning)',
        'accent-danger': 'var(--accent-danger)',
        'terminal-green':'var(--terminal-green)',
        'terminal-bg':  'var(--terminal-bg)',
      },
      fontFamily: {
        sans: ['var(--font-sans)'],
        mono: ['var(--font-mono)'],
      },
      borderRadius: {
        DEFAULT: 'var(--radius)',
      },
    },
  },
  plugins: [],
}
