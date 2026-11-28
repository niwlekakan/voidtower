import { render, screen } from '@testing-library/react'
import { MemoryRouter } from 'react-router-dom'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { api } from '@/api/client'
import type { CmdbAsset, Role } from '@/api/types'
import { useAuthStore } from '@/store/auth'
import { DOCK_ITEMS } from './AiosDock'
import { NATIVE_PANEL_REGISTRY, PANEL_REGISTRY, SPECIAL_PANELS } from './AiosLayout'

// Importing the layout pulls in every page it can render; the terminal page
// loads @xterm/xterm, which needs a canvas jsdom does not implement. Nothing in
// this file renders it, so stub it to keep the run output clean.
vi.mock('@/pages/Terminal', () => ({ default: () => null }))
vi.mock('@/components/ui/MiniTerminal', () => ({ MiniTerminal: () => null, default: () => null }))

// The void-mode dock opens panels by key through `PanelContent`, which renders
// `Unknown page: <key>` when neither registry nor the special-panel table owns
// the key. That failure is silent in the UI, so pin the whole class: every
// dock item must resolve.

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

describe('void-mode dock reachability', () => {
  beforeEach(() => {
    vi.restoreAllMocks()
  })

  it('resolves every dock key to a panel component', () => {
    const unresolved = DOCK_ITEMS.filter(
      ({ key }) => !(key in SPECIAL_PANELS) && !(key in NATIVE_PANEL_REGISTRY) && !(key in PANEL_REGISTRY),
    ).map(({ key }) => key)

    // A key listed here would open a panel that renders `Unknown page: <key>`.
    expect(unresolved).toEqual([])
  })

  it('fails the inventory panel closed for a role the server refuses, issuing no CMDB read', async () => {
    const assets = vi.spyOn(api.cmdb, 'assets').mockResolvedValue({ assets: [asset], limit: 50, offset: 0 })
    const discoveries = vi.spyOn(api.cmdb, 'discoveries').mockResolvedValue({ discoveries: [], limit: 50, offset: 0 })
    const InventoryPanel = PANEL_REGISTRY.inventory
    signInAs('viewer')

    render(
      <MemoryRouter>
        <InventoryPanel />
      </MemoryRouter>,
    )

    expect(await screen.findByText('Access restricted')).toBeInTheDocument()
    expect(screen.queryByRole('heading', { level: 1, name: 'Inventory' })).not.toBeInTheDocument()
    expect(assets).not.toHaveBeenCalled()
    expect(discoveries).not.toHaveBeenCalled()
  })

  it('renders the inventory surface inside the panel for an operator', async () => {
    vi.spyOn(api.cmdb, 'assets').mockResolvedValue({ assets: [asset], limit: 50, offset: 0 })
    vi.spyOn(api.cmdb, 'discoveries').mockResolvedValue({ discoveries: [], limit: 50, offset: 0 })
    const InventoryPanel = PANEL_REGISTRY.inventory
    signInAs('operator')

    render(
      <MemoryRouter>
        <InventoryPanel />
      </MemoryRouter>,
    )

    expect(await screen.findByRole('heading', { level: 1, name: 'Inventory' })).toBeInTheDocument()
    expect(await screen.findByText('edge-01')).toBeInTheDocument()
    expect(screen.queryByText('Access restricted')).not.toBeInTheDocument()
  })
})
