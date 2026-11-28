import { afterEach, describe, expect, it, vi } from 'vitest'
import { fireEvent, render, screen, waitFor } from '@testing-library/react'
const { apiAction } = vi.hoisted(() => ({ apiAction: vi.fn() }))
import NativeServicesPanel from './services'

vi.mock('@/api/client', () => ({
  api: { services: { action: apiAction } },
  createIdempotencyKey: () => '00000000-0000-4000-8000-000000000000',
}))
vi.mock('@/hooks/useDurableJobTracker', () => ({
  useDurableJobTracker: () => ({
    trackedJob: null,
    trackedLabel: null,
    tracking: false,
    track: vi.fn(),
  }),
}))

afterEach(() => {
  vi.unstubAllGlobals()
  vi.clearAllMocks()
})

describe('NativeServicesPanel restart errors', () => {
  it('shows a rejected restart request instead of dropping the promise', async () => {
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue({
      ok: true,
      json: async () => ({ services: [{ name: 'fixture.service', active: 'active', enabled: true }] }),
    }))
    apiAction.mockRejectedValueOnce(new Error('Policy denied restart'))

    render(<NativeServicesPanel />)
    fireEvent.click(await screen.findByTitle('Restart'))

    expect(await screen.findByRole('alert')).toHaveTextContent('Policy denied restart')
  })
  it('reuses the same idempotency key when retrying an ambiguous request', async () => {
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue({ ok: true, json: async () => ({ services: [{ name: 'fixture.service', active: 'active', enabled: true }] }) }))
    apiAction.mockRejectedValueOnce(new Error('Network response was lost'))
    apiAction.mockResolvedValueOnce({ job: { id: 'accepted-job', state: 'queued' } })
    render(<NativeServicesPanel />)
    fireEvent.click(await screen.findByTitle('Restart'))
    expect(await screen.findByRole('alert')).toHaveTextContent('Network response was lost')
    fireEvent.click(screen.getByTitle('Restart'))
    await waitFor(() => expect(apiAction).toHaveBeenCalledTimes(2))
    const firstKey = apiAction.mock.calls[0][2]
    expect(firstKey).toMatch(/^[A-Za-z0-9][A-Za-z0-9._:-]{0,127}$/)
    expect(apiAction.mock.calls[1][2]).toBe(firstKey)
  })

  it('prevents a second restart submission while the first request is pending', async () => {
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue({ ok: true, json: async () => ({ services: [{ name: 'fixture.service', active: 'active', enabled: true }] }) }))
    let resolveAction!: (value: unknown) => void
    apiAction.mockReturnValueOnce(new Promise(resolve => { resolveAction = resolve }))
    render(<NativeServicesPanel />)
    fireEvent.click(await screen.findByTitle('Restart'))
    await waitFor(() => expect(apiAction).toHaveBeenCalledTimes(1))
    expect(screen.getByTitle('Restart')).toBeDisabled()
    fireEvent.click(screen.getByTitle('Restart'))
    expect(apiAction).toHaveBeenCalledTimes(1)
    resolveAction({ job: { id: 'accepted-job', state: 'queued' } })
  })
})
