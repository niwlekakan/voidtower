import { fireEvent, render, screen, waitFor } from '@testing-library/react'
import { MemoryRouter, Route, Routes } from 'react-router-dom'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { api } from '@/api/client'
import { durableApproval, durableJob } from '@/test/operationFixtures'
import { useApprovalDetail, useApprovalList, useJobDetail } from '@/hooks/useOperationRecords'
import ApprovalsPage from './Approvals'

vi.mock('@/hooks/useOperationRecords', () => ({
  useApprovalDetail: vi.fn(),
  useApprovalList: vi.fn(),
  useJobDetail: vi.fn(),
}))

const approval = durableApproval()
const job = durableJob({ id: approval.job_id, state: 'awaiting_approval', approval_id: approval.id })
const refreshApproval = vi.fn().mockResolvedValue(approval)
const refreshJob = vi.fn().mockResolvedValue(job)
const accept = vi.fn()

function polling(data: unknown, refresh: () => Promise<unknown>) {
  return { data, loading: false, refreshing: false, stale: false, error: null, deadlineReached: false, refresh, accept }
}

describe('ApprovalsPage decisions', () => {
  beforeEach(() => {
    vi.mocked(useApprovalDetail).mockReturnValue(polling(approval, refreshApproval) as ReturnType<typeof useApprovalDetail>)
    vi.mocked(useApprovalList).mockReturnValue(polling([], refreshApproval) as ReturnType<typeof useApprovalList>)
    vi.mocked(useJobDetail).mockReturnValue(polling(job, refreshJob) as ReturnType<typeof useJobDetail>)
    vi.spyOn(window, 'confirm').mockReturnValue(true)
    refreshApproval.mockClear()
    refreshJob.mockClear()
    accept.mockClear()
  })

  it('approves the exact URL record once and refetches approval and job', async () => {
    const approve = vi.spyOn(api.approvals, 'approve').mockResolvedValue({ job: durableJob({ id: job.id, state: 'queued' }) })
    render(
      <MemoryRouter initialEntries={[`/approvals/${approval.id}`]} future={{ v7_startTransition: true, v7_relativeSplatPath: true }}>
        <Routes><Route path="/approvals/:id" element={<ApprovalsPage />} /></Routes>
      </MemoryRouter>,
    )
    fireEvent.change(screen.getByPlaceholderText('Optional decision comment'), { target: { value: ' reviewed ' } })
    fireEvent.click(screen.getByRole('button', { name: 'Approve exact plan' }))
    await waitFor(() => expect(approve).toHaveBeenCalledWith(approval.id, ' reviewed '))
    expect(approve).toHaveBeenCalledTimes(1)
    expect(accept).toHaveBeenCalledWith(expect.objectContaining({ state: 'queued' }))
    expect(refreshApproval).toHaveBeenCalledTimes(1)
    expect(refreshJob).toHaveBeenCalledTimes(1)
  })
})
