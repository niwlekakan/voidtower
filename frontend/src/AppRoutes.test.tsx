import { render, screen, waitFor } from '@testing-library/react'
import { MemoryRouter } from 'react-router-dom'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { api } from '@/api/client'
import type { CmdbAsset, Role } from '@/api/types'
import { useAuthStore } from '@/store/auth'
import { useNavConfigStore } from '@/store/navConfig'
import AppRoutes from './AppRoutes'

// The route table is what this file pins; the shells are replaced by inert
// outlets so a route-level role gate can be observed without a full layout.
vi.mock('@/components/layout/AppLayout', async () => {
  const { Outlet } = await vi.importActual<typeof import('react-router-dom')>('react-router-dom')
  return { default: () => <div data-testid="default-shell"><Outlet /></div> }
})

vi.mock('@/aios/AiosLayout', async () => {
  const { Outlet } = await vi.importActual<typeof import('react-router-dom')>('react-router-dom')
  return { default: () => <div data-testid="void-shell"><Outlet /></div> }
})

const asset: CmdbAsset = {
  resource_id: '4d1a6d4e-0a5f-4f1b-9a30-8f3f7ad1c101',
  resource_kind: 'host',
  display_name: 'edge-01',
  asset_id: 'VT-000042',
  class_key: 'compute',
  type_key: 'server',
  subtype: null,
  friendly_name: null,
  description: null,
  manufacturer: 'Example',
  model: 'R650',
  serial_number: 'SN-SYNTHETIC-1',
  part_number: null,
  lifecycle_status: 'in_service',
  discovery_status: 'observed',
  condition_status: 'good',
  location_id: null,
  first_seen_at: 1_764_000_000,
  last_seen_at: 1_764_003_600,
  metadata_json: '{}',
  notes: '',
  revision: 3,
  created_at: 1_764_000_000,
  updated_at: 1_764_003_600,
}

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

function renderAt(path: string) {
  return render(
    <MemoryRouter initialEntries={[path]}>
      <AppRoutes />
    </MemoryRouter>,
  )
}

describe('authed route table: /inventory', () => {
  beforeEach(() => {
    vi.restoreAllMocks()
    useNavConfigStore.setState({ items: [], navGroups: [], hydrated: true })
  })

  it('refuses the CMDB inventory route to a viewer before any CMDB read is issued', async () => {
    const assets = vi.spyOn(api.cmdb, 'assets').mockResolvedValue({ assets: [asset], limit: 50, offset: 0 })
    const discoveries = vi.spyOn(api.cmdb, 'discoveries').mockResolvedValue({ discoveries: [], limit: 50, offset: 0 })
    signInAs('viewer')

    renderAt('/inventory')

    expect(await screen.findByText('Access restricted')).toBeInTheDocument()
    expect(screen.queryByRole('heading', { level: 1, name: 'Inventory' })).not.toBeInTheDocument()
    expect(assets).not.toHaveBeenCalled()
    expect(discoveries).not.toHaveBeenCalled()
  })

  it('serves the CMDB inventory route to an operator inside the default shell', async () => {
    vi.spyOn(api.cmdb, 'assets').mockResolvedValue({ assets: [asset], limit: 50, offset: 0 })
    vi.spyOn(api.cmdb, 'discoveries').mockResolvedValue({ discoveries: [], limit: 50, offset: 0 })
    signInAs('operator')

    renderAt('/inventory')

    expect(await screen.findByRole('heading', { level: 1, name: 'Inventory' })).toBeInTheDocument()
    expect(await screen.findByText('edge-01')).toBeInTheDocument()
    expect(screen.getByTestId('default-shell')).toBeInTheDocument()
    await waitFor(() => expect(screen.queryByText('Access restricted')).not.toBeInTheDocument())
  })
})
