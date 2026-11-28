import { fireEvent, render, screen, waitFor } from '@testing-library/react'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import ServicesPage from './Services'

const mocks = vi.hoisted(() => ({
  list: vi.fn(),
  action: vi.fn(),
  logs: vi.fn(),
  notifyError: vi.fn(),
  track: vi.fn(),
}))

vi.mock('@/api/client', () => ({
  api: { services: { list: mocks.list, action: mocks.action, logs: mocks.logs } },
  createIdempotencyKey: () => '00000000-0000-4000-8000-000000000000',
  ApiClientError: class ApiClientError extends Error {},
}))
vi.mock('@/store/auth', () => ({ useAuthStore: (selector: (state: unknown) => unknown) => selector({ user: { role: 'admin' } }) }))
vi.mock('@/store/filters', () => ({ useFiltersStore: (selector: (state: unknown) => unknown) => selector({ globalTag: null }) }))
vi.mock('@/store/notifications', () => ({ notify: { error: mocks.notifyError } }))
vi.mock('@/hooks/useDurableJobTracker', () => ({ useDurableJobTracker: () => ({ trackedJob: null, trackedLabel: null, tracking: false, track: mocks.track }) }))
vi.mock('@/components/ui/StatusBadge', () => ({ default: () => <span>active</span> }))
vi.mock('@/components/ui/Button', () => ({ default: ({ children, onClick, disabled, loading, title }: any) => <button title={title} disabled={disabled || loading} onClick={onClick}>{children}</button> }))
vi.mock('@/components/ui/ConfirmDialog', () => ({ default: ({ onConfirm, onCancel, confirmLabel }: any) => <div><button onClick={onConfirm}>{confirmLabel}</button><button onClick={onCancel}>Cancel dialog</button></div> }))
vi.mock('@/components/ui/LogViewer', () => ({ default: () => null }))
vi.mock('@/components/ui/TagPill', () => ({ TagPill: () => null, TagPopover: () => null }))
vi.mock('@/components/ui/SendToOdysseus', () => ({ default: () => null }))
vi.mock('@/components/ui/DurableJobNotice', () => ({ default: () => null }))

beforeEach(() => {
  vi.clearAllMocks()
  mocks.list.mockResolvedValue({ services: [{ name: 'fixture.service', active_state: 'active', sub_state: 'running', enabled: true }], systemd_available: true })
  mocks.logs.mockResolvedValue({ lines: [] })
})

describe('ServicesPage restart idempotency', () => {
  it('reuses the key after an ambiguous request failure', async () => {
    mocks.action.mockRejectedValueOnce(new Error('Network response was lost'))
    mocks.action.mockResolvedValueOnce({ job: { id: 'job-1', state: 'queued' } })
    render(<ServicesPage />)
    fireEvent.click((await screen.findAllByTitle('Restart'))[0])
    fireEvent.click(await screen.findByRole('button', { name: 'restart' }))
    await waitFor(() => expect(mocks.notifyError).toHaveBeenCalled())

    fireEvent.click((await screen.findAllByTitle('Restart'))[0])
    fireEvent.click(await screen.findByRole('button', { name: 'restart' }))
    await waitFor(() => expect(mocks.action).toHaveBeenCalledTimes(2))
    expect(mocks.action.mock.calls[1][2]).toBe(mocks.action.mock.calls[0][2])
    expect(mocks.track).toHaveBeenCalledTimes(1)
  })
})
