import { render, screen } from '@testing-library/react'
import { MemoryRouter } from 'react-router-dom'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { useAuthStore } from '@/store/auth'
import { useCmdPaletteStore } from '@/store/cmdpalette'
import type { Role } from '@/api/types'
import CommandPalette from './CommandPalette'

vi.mock('@/api/client', () => ({
  api: { auth: { logout: vi.fn(async () => ({})) } },
}))

// cmdk measures its list with ResizeObserver and scrolls the active item;
// jsdom implements neither.
class ResizeObserverStub {
  observe() {}
  unobserve() {}
  disconnect() {}
}
vi.stubGlobal('ResizeObserver', ResizeObserverStub)
Element.prototype.scrollIntoView = () => {}

function signInAs(role: Role) {
  useAuthStore.setState({
    status: 'authenticated',
    user: {
      id: 'u-1',
      username: 'synthetic-user',
      role,
      force_password_change: false,
      totp_enabled: true,
      mfa_required: false,
    },
  })
}

function openPalette() {
  useCmdPaletteStore.setState({ open: true, query: '' })
  return render(
    <MemoryRouter>
      <CommandPalette />
    </MemoryRouter>,
  )
}

describe('Command palette navigation commands', () => {
  beforeEach(() => {
    vi.restoreAllMocks()
  })

  it('offers the CMDB inventory surface to an operator', () => {
    signInAs('operator')
    openPalette()

    expect(screen.getByText('Go to Inventory')).toBeInTheDocument()
  })

  it('does not offer the CMDB inventory surface to a viewer', () => {
    signInAs('viewer')
    openPalette()

    expect(screen.queryByText('Go to Inventory')).not.toBeInTheDocument()
    expect(screen.getByText('Go to Dashboard')).toBeInTheDocument()
  })
})
