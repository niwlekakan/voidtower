import { afterEach, describe, expect, it, vi } from 'vitest'
import { api } from './client'

function ok(body: unknown): Response {
  return new Response(JSON.stringify(body), { status: 200, headers: { 'Content-Type': 'application/json' } })
}

describe('durable operation API client', () => {
  afterEach(() => vi.unstubAllGlobals())

  it('encodes list/detail/cancel and idempotency lookup requests', async () => {
    const envelope = { schema_version: 1, resource_id: 'resource-1', action: 'container.start', job: {} }
    const fetch = vi.fn()
      .mockResolvedValueOnce(ok({ jobs: [] }))
      .mockResolvedValueOnce(ok(envelope))
      .mockResolvedValueOnce(ok(envelope))
      .mockResolvedValueOnce(ok(envelope))
    vi.stubGlobal('fetch', fetch)
    await api.operationJobs.list(25)
    await api.operationJobs.get('job/id')
    await api.operationJobs.cancel('job/id')
    await api.operationJobs.getByIdempotency('key.with:part')
    expect(fetch.mock.calls[0][0]).toBe('/api/jobs?limit=25')
    expect(fetch.mock.calls[1][0]).toBe('/api/jobs/job%2Fid')
    expect(fetch.mock.calls[2][0]).toBe('/api/jobs/job%2Fid/cancel')
    expect(fetch.mock.calls[2][1]).toMatchObject({
      method: 'POST',
    })
    expect(fetch.mock.calls[3][0]).toBe('/api/jobs/by-idempotency/key.with%3Apart')
    expect(fetch.mock.calls[3][1]).toMatchObject({
      credentials: 'include',
      headers: {
        'Content-Type': 'application/json',
        'x-voidtower-api-version': '1',
      },
    })
  })

  it('rejects invalid idempotency lookup keys before making a request', async () => {
    const fetch = vi.fn()
    vi.stubGlobal('fetch', fetch)

    await expect(api.operationJobs.getByIdempotency('bad/key'))
      .rejects.toMatchObject({ code: 'invalid_idempotency_key', status: 400 })
    expect(fetch).not.toHaveBeenCalled()
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
