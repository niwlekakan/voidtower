import { render, screen } from '@testing-library/react'
import { MemoryRouter } from 'react-router-dom'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { api } from '@/api/client'
import type { Role } from '@/api/types'
import { useAuthStore } from '@/store/auth'
import { useNavConfigStore } from '@/store/navConfig'
import Sidebar from './Sidebar'

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

describe('Sidebar navigation registration', () => {
  beforeEach(() => {
    vi.restoreAllMocks()
    useNavConfigStore.setState({ items: [], navGroups: [], hydrated: true })
    vi.spyOn(api.tabs, 'list').mockResolvedValue([])
    vi.stubGlobal(
      'fetch',
      vi.fn(async () => ({ ok: false, json: async () => null }) as unknown as Response),
    )
  })

  it('reaches the CMDB inventory surface from the default shell for an operator', () => {
    signInAs('operator')
    render(
      <MemoryRouter>
        <Sidebar />
      </MemoryRouter>,
    )

    const link = screen.getByRole('link', { name: 'Inventory' })
    expect(link).toHaveAttribute('href', '/inventory')
  })

  it('hides the CMDB inventory surface from a viewer whose reads the server refuses', () => {
    signInAs('viewer')
    render(
      <MemoryRouter>
        <Sidebar />
      </MemoryRouter>,
    )

    expect(screen.queryByRole('link', { name: 'Inventory' })).not.toBeInTheDocument()
    // The rest of the Resources group is untouched by the gate.
    expect(screen.getByRole('link', { name: 'Containers' })).toBeInTheDocument()
  })
})
