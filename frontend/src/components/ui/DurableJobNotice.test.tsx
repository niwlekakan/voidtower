import { render, screen } from '@testing-library/react'
import { MemoryRouter } from 'react-router-dom'
import { describe, expect, it } from 'vitest'
import { durableJob } from '@/test/operationFixtures'
import DurableJobNotice from './DurableJobNotice'

describe('DurableJobNotice', () => {
  it('links the local record to canonical shared job detail', () => {
    const job = durableJob()
    render(<MemoryRouter future={{ v7_startTransition: true, v7_relativeSplatPath: true }}><DurableJobNotice job={job} label="Restart web" tracking /></MemoryRouter>)
    expect(screen.getByRole('link', { name: job.id.slice(0, 8) })).toHaveAttribute('href', `/jobs/${job.id}`)
  })
})
