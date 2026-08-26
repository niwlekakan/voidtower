import { render, screen } from '@testing-library/react'
import { describe, expect, it } from 'vitest'
import { durableJob } from '@/test/operationFixtures'
import { BoundedValue, JobDetailView } from './OperationViews'

describe('bounded operation presentation', () => {
  it('masks secret-like fields and truncates long strings', () => {
    render(<BoundedValue value={{ api_token: 'do-not-show', safe: 'x'.repeat(550) }} />)
    expect(screen.getByText('[redacted]')).toBeInTheDocument()
    expect(screen.queryByText('do-not-show')).not.toBeInTheDocument()
    expect(screen.getByText(/\[truncated\]$/)).toHaveTextContent('x'.repeat(500))
  })

  it('renders typed plans and public errors without raw HTML', () => {
    const job = durableJob({
      state: 'failed',
      error: { code: 'provider_failed', message: '<img src=x onerror=alert(1)>', retryable: false, job_id: null },
    })
    const { container } = render(<JobDetailView job={job} />)
    expect(screen.getByText('Restart web')).toBeInTheDocument()
    expect(screen.getByText('<img src=x onerror=alert(1)>')).toBeInTheDocument()
    expect(container.querySelector('img')).toBeNull()
  })
})
