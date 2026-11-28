import { useCallback, useEffect, useMemo, useState } from 'react'
import { Boxes, ChevronLeft, ChevronRight, RefreshCw, Search } from 'lucide-react'
import { api, ApiClientError } from '@/api/client'
import type { CmdbAsset, CmdbDiscovery } from '@/api/types'

/**
 * The HTTP layer rejects a `limit` outside 1..200 with 400 (the domain query
 * clamps it instead) and neither list read returns a total, so the page must
 * never present a fetched page as an authoritative count. It steps through
 * explicit windows and only claims a remainder when a window came back full.
 */
export const PAGE_SIZE = 50

export function formatSeenAt(seconds: number | null | undefined): string {
  if (typeof seconds !== 'number' || !Number.isFinite(seconds) || seconds <= 0) return 'Never observed'
  return new Date(seconds * 1000).toLocaleString()
}

export function pageRangeLabel(offset: number, count: number): string {
  const range = `Showing ${offset + 1}–${offset + count}`
  // A full window means more rows may exist; a short window ends the list.
  return count === PAGE_SIZE ? `${range} · more may exist` : range
}

function DiscoveryStateBadge({ state }: { state: string }) {
  const review = state === 'review'
  const label = review ? 'Awaiting review' : state === 'ignored' ? 'Ignored' : state
  const color = review ? 'var(--accent-warning, #f59e0b)' : 'var(--text-muted)'
  return (
    <span
      className="inline-flex items-center rounded px-2 py-0.5 text-[10px] font-semibold uppercase tracking-[0.08em]"
      style={{ color, border: `1px solid color-mix(in srgb, ${color} 45%, transparent)` }}
    >
      {label}
    </span>
  )
}

function PageControls({
  scope,
  offset,
  count,
  disabled,
  onPrevious,
  onNext,
}: {
  scope: string
  offset: number
  count: number
  disabled: boolean
  onPrevious: () => void
  onNext: () => void
}) {
  const hasPrevious = offset > 0
  const hasNext = count === PAGE_SIZE
  if (count === 0 && !hasPrevious) return null
  return (
    <div className="flex items-center gap-2">
      {hasPrevious && (
        <button
          onClick={onPrevious}
          disabled={disabled}
          aria-label={`Previous page of ${scope}`}
          className="inline-flex items-center gap-1 rounded px-2 py-1 text-xs disabled:opacity-40"
          style={{ background: 'var(--bg-surface)', border: '1px solid var(--border-subtle)', color: 'var(--text-secondary)' }}
        >
          <ChevronLeft size={13} /> Previous
        </button>
      )}
      {hasNext && (
        <button
          onClick={onNext}
          disabled={disabled}
          aria-label={`Next page of ${scope}`}
          className="inline-flex items-center gap-1 rounded px-2 py-1 text-xs disabled:opacity-40"
          style={{ background: 'var(--bg-surface)', border: '1px solid var(--border-subtle)', color: 'var(--text-secondary)' }}
        >
          Next <ChevronRight size={13} />
        </button>
      )}
    </div>
  )
}

function assetMatches(asset: CmdbAsset, query: string): boolean {
  if (!query) return true
  const haystack = [
    asset.display_name,
    asset.asset_id,
    asset.class_key,
    asset.type_key,
    asset.subtype ?? '',
    asset.serial_number ?? '',
    asset.resource_id,
  ]
  return haystack.some((value) => value.toLowerCase().includes(query))
}

export default function InventoryPage() {
  const [assets, setAssets] = useState<CmdbAsset[]>([])
  const [discoveries, setDiscoveries] = useState<CmdbDiscovery[]>([])
  const [assetOffset, setAssetOffset] = useState(0)
  const [discoveryOffset, setDiscoveryOffset] = useState(0)
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState<string | null>(null)
  const [query, setQuery] = useState('')

  const load = useCallback(async () => {
    setLoading(true)
    setError(null)
    try {
      // Both reads are required for this surface; a partial read is reported as
      // a failure rather than rendered as an empty projection.
      const [assetPage, discoveryPage] = await Promise.all([
        api.cmdb.assets(PAGE_SIZE, assetOffset),
        api.cmdb.discoveries(PAGE_SIZE, discoveryOffset),
      ])
      setAssets(assetPage.assets)
      setDiscoveries(discoveryPage.discoveries)
    } catch (failure) {
      setAssets([])
      setDiscoveries([])
      setError(failure instanceof ApiClientError ? failure.message : 'Inventory could not be loaded')
    } finally {
      setLoading(false)
    }
  }, [assetOffset, discoveryOffset])

  useEffect(() => { void load() }, [load])

  const normalizedQuery = query.trim().toLowerCase()
  const visibleAssets = useMemo(
    () => assets.filter((asset) => assetMatches(asset, normalizedQuery)),
    [assets, normalizedQuery],
  )

  return (
    <div className="mx-auto max-w-6xl space-y-5">
      <header className="flex flex-wrap items-end justify-between gap-4">
        <div>
          <div className="text-[10px] font-semibold uppercase tracking-[0.16em]" style={{ color: 'var(--accent-primary)' }} aria-hidden="true">
            Canonical resources
          </div>
          <h1 className="mt-1 text-2xl font-semibold" style={{ color: 'var(--text-primary)' }}>Inventory</h1>
          <p className="mt-1 max-w-2xl text-sm" style={{ color: 'var(--text-secondary)' }}>
            Every row below is a projection over the canonical resource UUID. Administrator-owned
            fields stay authoritative; discovery evidence never overwrites them.
          </p>
        </div>
        <button
          onClick={() => void load()}
          disabled={loading}
          className="inline-flex items-center gap-1.5 rounded px-3 py-1.5 text-xs disabled:opacity-40"
          style={{ background: 'var(--bg-panel)', border: '1px solid var(--border-subtle)', color: 'var(--text-secondary)' }}
        >
          <RefreshCw size={13} className={loading ? 'animate-spin' : ''} /> Refresh
        </button>
      </header>

      {loading && (
        <div role="status" className="rounded-lg px-4 py-3 text-sm" style={{ background: 'var(--bg-panel)', color: 'var(--text-muted)' }}>
          Loading inventory…
        </div>
      )}

      {error && (
        <div role="alert" className="rounded-lg px-4 py-3 text-sm" style={{ background: 'var(--bg-panel)', border: '1px solid color-mix(in srgb, var(--accent-danger) 45%, transparent)', color: 'var(--text-primary)' }}>
          <div className="font-semibold" style={{ color: 'var(--accent-danger)' }}>Inventory could not be loaded</div>
          <p className="mt-1" style={{ color: 'var(--text-secondary)' }}>{error}</p>
          <button
            onClick={() => void load()}
            disabled={loading}
            className="mt-3 inline-flex items-center gap-1.5 rounded px-3 py-1.5 text-xs font-semibold disabled:opacity-40"
            style={{ background: 'var(--bg-surface)', border: '1px solid var(--border-subtle)', color: 'var(--text-primary)' }}
          >
            Retry
          </button>
        </div>
      )}

      {!error && !loading && (
        <>
          <section className="space-y-3 rounded-lg p-4" style={{ background: 'var(--bg-panel)', border: '1px solid var(--border-subtle)' }} aria-labelledby="inventory-assets-heading">
            <div className="flex flex-wrap items-center justify-between gap-3">
              <div className="flex items-center gap-2">
                <Boxes size={15} style={{ color: 'var(--accent-primary)' }} />
                <h2 id="inventory-assets-heading" className="text-sm font-semibold" style={{ color: 'var(--text-primary)' }}>Canonical resources</h2>
              </div>
              <label className="inline-flex items-center gap-2 text-xs" style={{ color: 'var(--text-muted)' }}>
                <Search size={13} aria-hidden="true" />
                <span className="sr-only">Filter resources on this page</span>
                <input
                  type="search"
                  aria-label="Filter resources on this page"
                  value={query}
                  onChange={(event) => setQuery(event.target.value)}
                  placeholder="Filter this page"
                  maxLength={120}
                  className="w-56 rounded px-2 py-1.5 text-xs outline-none"
                  style={{ background: 'var(--bg-surface)', border: '1px solid var(--border-subtle)', color: 'var(--text-primary)' }}
                />
              </label>
            </div>

            {assets.length > 0 && (
              <div className="flex flex-wrap items-center justify-between gap-2">
                <p className="text-xs" style={{ color: 'var(--text-muted)' }}>{pageRangeLabel(assetOffset, assets.length)}</p>
                <PageControls
                  scope="resources"
                  offset={assetOffset}
                  count={assets.length}
                  disabled={loading}
                  onPrevious={() => setAssetOffset((current) => Math.max(0, current - PAGE_SIZE))}
                  onNext={() => setAssetOffset((current) => current + PAGE_SIZE)}
                />
              </div>
            )}

            {assets.length === 0 ? (
              <div className="rounded px-4 py-8 text-center text-sm" style={{ background: 'var(--bg-surface)', color: 'var(--text-muted)' }}>
                {assetOffset === 0 ? 'No canonical resources yet' : 'No more canonical resources'}
              </div>
            ) : visibleAssets.length === 0 ? (
              <div className="rounded px-4 py-8 text-center text-sm" style={{ background: 'var(--bg-surface)', color: 'var(--text-muted)' }}>
                No resource on this page matches this filter
              </div>
            ) : (
              <div className="overflow-x-auto">
                <table className="w-full text-left text-xs">
                  <caption className="sr-only">Canonical resources with their administrator-owned and observed state</caption>
                  <thead>
                    <tr style={{ color: 'var(--text-muted)' }}>
                      <th scope="col" className="py-2 pr-3 font-semibold">Name</th>
                      <th scope="col" className="py-2 pr-3 font-semibold">Asset ID</th>
                      <th scope="col" className="py-2 pr-3 font-semibold">Class / type</th>
                      <th scope="col" className="py-2 pr-3 font-semibold">Lifecycle</th>
                      <th scope="col" className="py-2 pr-3 font-semibold">Discovery</th>
                      <th scope="col" className="py-2 font-semibold">Last seen</th>
                    </tr>
                  </thead>
                  <tbody>
                    {visibleAssets.map((asset) => (
                      <tr key={asset.resource_id} style={{ borderTop: '1px solid var(--border-subtle)' }}>
                        <td className="py-2 pr-3 font-medium" style={{ color: 'var(--text-primary)' }}>{asset.display_name}</td>
                        <td className="py-2 pr-3 font-mono" style={{ color: 'var(--text-secondary)' }}>{asset.asset_id}</td>
                        <td className="py-2 pr-3" style={{ color: 'var(--text-secondary)' }}>{`${asset.class_key} / ${asset.type_key}`}</td>
                        <td className="py-2 pr-3" style={{ color: 'var(--text-secondary)' }}>{asset.lifecycle_status}</td>
                        <td className="py-2 pr-3" style={{ color: 'var(--text-secondary)' }}>{asset.discovery_status}</td>
                        <td className="py-2" style={{ color: 'var(--text-secondary)' }}>{formatSeenAt(asset.last_seen_at)}</td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>
            )}
          </section>

          <section className="space-y-3 rounded-lg p-4" style={{ background: 'var(--bg-panel)', border: '1px solid var(--border-subtle)' }} aria-labelledby="inventory-discoveries-heading">
            <div className="flex items-center gap-2">
              <h2 id="inventory-discoveries-heading" className="text-sm font-semibold" style={{ color: 'var(--text-primary)' }}>Discovery review queue</h2>
            </div>
            <p className="text-xs" style={{ color: 'var(--text-muted)' }}>
              Discovery observations that did not link to a canonical resource by strong evidence.
              The list includes entries already marked ignored, and the state badge on each row says
              which is which. Linking, registering, or ignoring a discovery is an administrator
              decision and is not available in this read-only surface.
            </p>

            {discoveries.length > 0 && (
              <div className="flex flex-wrap items-center justify-between gap-2">
                <p className="text-xs" style={{ color: 'var(--text-muted)' }}>{pageRangeLabel(discoveryOffset, discoveries.length)}</p>
                <PageControls
                  scope="discoveries"
                  offset={discoveryOffset}
                  count={discoveries.length}
                  disabled={loading}
                  onPrevious={() => setDiscoveryOffset((current) => Math.max(0, current - PAGE_SIZE))}
                  onNext={() => setDiscoveryOffset((current) => current + PAGE_SIZE)}
                />
              </div>
            )}

            {discoveries.length === 0 ? (
              <div className="rounded px-4 py-8 text-center text-sm" style={{ background: 'var(--bg-surface)', color: 'var(--text-muted)' }}>
                {discoveryOffset === 0 ? 'No discoveries are waiting for a decision' : 'No more discoveries'}
              </div>
            ) : (
              <div className="overflow-x-auto">
                <table className="w-full text-left text-xs">
                  <caption className="sr-only">Discovery evidence listed for review, including entries already marked ignored</caption>
                  <thead>
                    <tr style={{ color: 'var(--text-muted)' }}>
                      <th scope="col" className="py-2 pr-3 font-semibold">Entity</th>
                      <th scope="col" className="py-2 pr-3 font-semibold">Provider</th>
                      <th scope="col" className="py-2 pr-3 font-semibold">State</th>
                      <th scope="col" className="py-2 pr-3 font-semibold">Evidence fingerprint</th>
                      <th scope="col" className="py-2 font-semibold">Last seen</th>
                    </tr>
                  </thead>
                  <tbody>
                    {discoveries.map((item) => (
                      <tr key={item.id} style={{ borderTop: '1px solid var(--border-subtle)' }}>
                        <td className="py-2 pr-3 font-mono" style={{ color: 'var(--text-primary)' }}>{item.entity_key}</td>
                        <td className="py-2 pr-3" style={{ color: 'var(--text-secondary)' }}>{item.provider}</td>
                        <td className="py-2 pr-3"><DiscoveryStateBadge state={item.state} /></td>
                        <td className="py-2 pr-3 font-mono" style={{ color: 'var(--text-muted)' }} title={item.fingerprint}>{item.fingerprint.slice(0, 12)}</td>
                        <td className="py-2" style={{ color: 'var(--text-secondary)' }}>{formatSeenAt(item.last_seen_at)}</td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>
            )}
          </section>
        </>
      )}
    </div>
  )
}
