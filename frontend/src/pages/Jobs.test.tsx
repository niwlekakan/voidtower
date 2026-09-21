import { fireEvent, render, screen, waitFor } from '@testing-library/react'
import { MemoryRouter, Route, Routes } from 'react-router-dom'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { api } from '@/api/client'
import { durableJob } from '@/test/operationFixtures'
import { useJobDetail, useJobList } from '@/hooks/useOperationRecords'
import JobsPage from './Jobs'

vi.mock('@/hooks/useOperationRecords', () => ({
  useJobDetail: vi.fn(),
  useJobList: vi.fn(),
}))

const job = durableJob({ state: 'running' })
const refresh = vi.fn().mockResolvedValue(job)
const accept = vi.fn()

function polling(data: unknown) {
  return { data, loading: false, refreshing: false, stale: false, error: null, deadlineReached: false, refresh, accept }
}

describe('JobsPage actions', () => {
  beforeEach(() => {
    vi.mocked(useJobDetail).mockReturnValue(polling(job) as ReturnType<typeof useJobDetail>)
    vi.mocked(useJobList).mockReturnValue(polling([]) as ReturnType<typeof useJobList>)
    vi.spyOn(window, 'confirm').mockReturnValue(true)
    refresh.mockClear()
    accept.mockClear()
  })

  it('sends one cancellation for the selected job and refetches authoritative detail', async () => {
    const cancel = vi.spyOn(api.operationJobs, 'cancel').mockResolvedValue({
      schema_version: 1,
      resource_id: job.resource.id,
      action: job.action,
      job: durableJob({ state: 'cancelled' }),
    })
    render(
      <MemoryRouter initialEntries={[`/jobs/${job.id}`]} future={{ v7_startTransition: true, v7_relativeSplatPath: true }}>
        <Routes><Route path="/jobs/:id" element={<JobsPage />} /></Routes>
      </MemoryRouter>,
    )
    fireEvent.click(screen.getByRole('button', { name: 'Cancel job' }))
    await waitFor(() => expect(cancel).toHaveBeenCalledWith(job.id))
    expect(cancel).toHaveBeenCalledTimes(1)
    expect(accept).toHaveBeenCalledWith(expect.objectContaining({ state: 'cancelled' }))
    expect(refresh).not.toHaveBeenCalled()
  })
})
