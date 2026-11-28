import { act, fireEvent, render, screen, waitFor } from '@testing-library/react'
import { MemoryRouter } from 'react-router-dom'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { api } from '@/api/client'
import type { CmdbAsset, CmdbDiscovery } from '@/api/types'
import InventoryPage, { PAGE_SIZE } from './Inventory'

const asset = (overrides: Partial<CmdbAsset> = {}): CmdbAsset => ({
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
  ...overrides,
})

const discovery = (overrides: Partial<CmdbDiscovery> = {}): CmdbDiscovery => ({
  id: '11111111-2222-4333-8444-555555555555',
  resource_id: null,
  source_resource_id: '99999999-8888-4777-8666-555555555555',
  provider: 'linux-agent',
  entity_key: 'disk:synthetic-nvme0',
  entity_type: 'physical_disk',
  schema_version: 1,
  identities: [],
  attributes: {},
  runtime: {},
  health: {},
  provider_observed_at: 1_764_003_000,
  received_at: 1_764_003_600,
  first_seen_at: 1_764_000_000,
  last_seen_at: 1_764_003_600,
  state: 'review',
  fingerprint: 'a'.repeat(64),
  ...overrides,
})

function mockLists(assets: CmdbAsset[], discoveries: CmdbDiscovery[]) {
  const assetsSpy = vi.spyOn(api.cmdb, 'assets').mockResolvedValue({ assets, limit: PAGE_SIZE, offset: 0 })
  const discoveriesSpy = vi
    .spyOn(api.cmdb, 'discoveries')
    .mockResolvedValue({ discoveries, limit: PAGE_SIZE, offset: 0 })
  return { assetsSpy, discoveriesSpy }
}

function renderPage() {
  return render(
    <MemoryRouter initialEntries={['/inventory']}>
      <InventoryPage />
    </MemoryRouter>,
  )
}

describe('InventoryPage', () => {
  beforeEach(() => {
    vi.restoreAllMocks()
  })

  it('renders canonical resources and the discovery review queue from the read-only CMDB API', async () => {
    const { assetsSpy, discoveriesSpy } = mockLists([asset()], [discovery()])
    renderPage()

    expect(await screen.findByRole('heading', { level: 1, name: 'Inventory' })).toBeInTheDocument()
    expect(await screen.findByText('edge-01')).toBeInTheDocument()
    expect(screen.getByText('VT-000042')).toBeInTheDocument()
    expect(screen.getByText('compute / server')).toBeInTheDocument()
    expect(screen.getByText('disk:synthetic-nvme0')).toBeInTheDocument()
    expect(screen.getByText('Awaiting review')).toBeInTheDocument()

    await waitFor(() => expect(assetsSpy).toHaveBeenCalledTimes(1))
    expect(discoveriesSpy).toHaveBeenCalledTimes(1)
    expect(assetsSpy).toHaveBeenCalledWith(PAGE_SIZE, 0)
  })

  it('separates an ignored discovery from one that needs a decision', async () => {
    mockLists(
      [asset()],
      [
        discovery({ id: 'review-1', entity_key: 'disk:needs-decision', state: 'review' }),
        discovery({ id: 'ignored-1', entity_key: 'disk:ignored-already', state: 'ignored' }),
      ],
    )
    renderPage()

    expect(await screen.findByText('disk:needs-decision')).toBeInTheDocument()
    expect(screen.getByText('disk:ignored-already')).toBeInTheDocument()
    expect(screen.getByText('Awaiting review')).toBeInTheDocument()
    expect(screen.getByText('Ignored')).toBeInTheDocument()
  })

  it('shows distinct empty states instead of pretending an empty list is a loaded one', async () => {
    mockLists([], [])
    renderPage()

    expect(await screen.findByText('No canonical resources yet')).toBeInTheDocument()
    expect(screen.getByText('No discoveries are waiting for a decision')).toBeInTheDocument()
  })

  it('never presents a capped page as an authoritative total', async () => {
    const fullPage = Array.from({ length: PAGE_SIZE }, (_, index) =>
      asset({
        resource_id: `f${String(index).padStart(35, '0')}`,
        asset_id: `VT-${String(index).padStart(6, '0')}`,
        display_name: `node-${index}`,
      }),
    )
    const assetsSpy = vi
      .spyOn(api.cmdb, 'assets')
      .mockResolvedValueOnce({ assets: fullPage, limit: PAGE_SIZE, offset: 0 })
      .mockResolvedValueOnce({
        assets: [asset({ resource_id: 'g'.repeat(36), asset_id: 'VT-900001', display_name: 'node-tail' })],
        limit: PAGE_SIZE,
        offset: PAGE_SIZE,
      })
    const discoveriesSpy = vi
      .spyOn(api.cmdb, 'discoveries')
      .mockResolvedValue({ discoveries: [], limit: PAGE_SIZE, offset: 0 })

    const { container } = renderPage()

    // A full page is disclosed as a window with a possible remainder, never as a count.
    expect(await screen.findByText(`Showing 1–${PAGE_SIZE} · more may exist`)).toBeInTheDocument()
    // No stat block is rendered, and nothing on the page claims a total: either
    // would turn a capped window back into an authoritative count.
    expect(container.querySelector('[data-stat]')).toBeNull()
    expect(screen.queryByText(/total/i)).not.toBeInTheDocument()

    fireEvent.click(screen.getByRole('button', { name: 'Next page of resources' }))

    expect(await screen.findByText('node-tail')).toBeInTheDocument()
    expect(assetsSpy).toHaveBeenLastCalledWith(PAGE_SIZE, PAGE_SIZE)
    expect(discoveriesSpy).toHaveBeenCalled()
    // A short page ends the list: no remainder claim and no forward control.
    expect(screen.getByText(`Showing ${PAGE_SIZE + 1}–${PAGE_SIZE + 1}`)).toBeInTheDocument()
    expect(screen.queryByRole('button', { name: 'Next page of resources' })).not.toBeInTheDocument()
    expect(screen.getByRole('button', { name: 'Previous page of resources' })).toBeInTheDocument()
  })

  it('filters the current page without claiming a global match count', async () => {
    mockLists([asset(), asset({ resource_id: 'b'.repeat(36), display_name: 'core-02', asset_id: 'VT-000043' })], [])
    renderPage()

    expect(await screen.findByText('edge-01')).toBeInTheDocument()
    fireEvent.change(screen.getByRole('searchbox', { name: 'Filter resources on this page' }), {
      target: { value: 'core' },
    })

    expect(screen.queryByText('edge-01')).not.toBeInTheDocument()
    expect(screen.getByText('core-02')).toBeInTheDocument()
  })

  it('reports a still-pending read as loading rather than as an empty projection', async () => {
    let releaseAssets: (value: { assets: CmdbAsset[]; limit: number; offset: number }) => void = () => {}
    let releaseDiscoveries: (value: { discoveries: CmdbDiscovery[]; limit: number; offset: number }) => void = () => {}
    vi.spyOn(api.cmdb, 'assets').mockReturnValue(
      new Promise((resolve) => {
        releaseAssets = resolve
      }),
    )
    vi.spyOn(api.cmdb, 'discoveries').mockReturnValue(
      new Promise((resolve) => {
        releaseDiscoveries = resolve
      }),
    )

    renderPage()

    const status = await screen.findByRole('status')
    expect(status).toHaveTextContent('Loading inventory…')
    expect(screen.queryByText('No canonical resources yet')).not.toBeInTheDocument()
    expect(screen.queryByText('No resource on this page matches this filter')).not.toBeInTheDocument()
    expect(screen.queryByText('No discoveries are waiting for a decision')).not.toBeInTheDocument()

    // A read that has not been paired with the other still counts as loading:
    // the surface never renders a half-loaded projection.
    await act(async () => {
      releaseAssets({ assets: [asset()], limit: PAGE_SIZE, offset: 0 })
    })
    expect(screen.getByRole('status')).toHaveTextContent('Loading inventory…')
    expect(screen.queryByText('edge-01')).not.toBeInTheDocument()

    releaseDiscoveries({ discoveries: [], limit: PAGE_SIZE, offset: 0 })
    await waitFor(() => expect(screen.queryByRole('status')).not.toBeInTheDocument())
    expect(await screen.findByText('edge-01')).toBeInTheDocument()
  })

  it('surfaces a load failure and refetches only when the operator asks', async () => {
    const failing = vi.spyOn(api.cmdb, 'assets').mockRejectedValueOnce(new Error('boom'))
    const discoveriesSpy = vi
      .spyOn(api.cmdb, 'discoveries')
      .mockResolvedValue({ discoveries: [], limit: PAGE_SIZE, offset: 0 })
    renderPage()

    const alert = await screen.findByRole('alert')
    expect(alert).toHaveTextContent('Inventory could not be loaded')
    expect(screen.queryByText('No canonical resources yet')).not.toBeInTheDocument()

    failing.mockResolvedValueOnce({ assets: [asset()], limit: PAGE_SIZE, offset: 0 })
    fireEvent.click(screen.getByRole('button', { name: 'Retry' }))

    expect(await screen.findByText('edge-01')).toBeInTheDocument()
    await waitFor(() => expect(failing).toHaveBeenCalledTimes(2))
    expect(discoveriesSpy).toHaveBeenCalledTimes(2)
  })

  it('never issues a mutation for this surface', async () => {
    const { assetsSpy, discoveriesSpy } = mockLists([asset()], [])
    renderPage()

    expect(await screen.findByText('edge-01')).toBeInTheDocument()
    expect(assetsSpy).toHaveBeenCalledTimes(1)
    expect(discoveriesSpy).toHaveBeenCalledTimes(1)
    expect(api.cmdb).not.toHaveProperty('link')
    expect(api.cmdb).not.toHaveProperty('register')
    expect(api.cmdb).not.toHaveProperty('ignore')
  })
})
