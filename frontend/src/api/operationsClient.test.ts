import { afterEach, describe, expect, it, vi } from 'vitest'
import { api } from './client'

function ok(body: unknown): Response {
  return new Response(JSON.stringify(body), { status: 200, headers: { 'Content-Type': 'application/json' } })
}

describe('durable operation API client', () => {
  afterEach(() => vi.unstubAllGlobals())

  it('encodes list/detail/cancel job requests', async () => {
    const fetch = vi.fn()
      .mockResolvedValueOnce(ok({ jobs: [] }))
      .mockResolvedValueOnce(ok({ schema_version: 1, resource_id: 'resource-1', action: 'container.start', job: {} }))
      .mockResolvedValueOnce(ok({ schema_version: 1, resource_id: 'resource-1', action: 'container.start', job: {} }))
    vi.stubGlobal('fetch', fetch)
    await api.operationJobs.list(25)
    await api.operationJobs.get('job/id')
    await api.operationJobs.cancel('job/id')
    expect(fetch.mock.calls[0][0]).toBe('/api/jobs?limit=25')
    expect(fetch.mock.calls[1][0]).toBe('/api/jobs/job%2Fid')
    expect(fetch.mock.calls[2][0]).toBe('/api/jobs/job%2Fid/cancel')
    expect(fetch.mock.calls[2][1]).toMatchObject({
      method: 'POST',
      headers: {
        'x-voidtower-api-version': '1',
      },
    })
  })

  it('omits absent approval status and sends one trimmed exact-record decision', async () => {
    const fetch = vi.fn()
      .mockResolvedValueOnce(ok({ approvals: [] }))
      .mockResolvedValueOnce(ok({ job: {} }))
    vi.stubGlobal('fetch', fetch)
    await api.approvals.list({ limit: 10 })
    await api.approvals.approve('approval/id', '  reviewed  ')
    expect(fetch.mock.calls[0][0]).toBe('/api/approvals?limit=10')
    expect(fetch.mock.calls[1][0]).toBe('/api/approvals/approval%2Fid/approve')
    expect(JSON.parse(fetch.mock.calls[1][1].body)).toEqual({ comment: 'reviewed' })
    expect(fetch).toHaveBeenCalledTimes(2)
  })
})
