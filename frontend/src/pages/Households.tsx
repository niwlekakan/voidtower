import { useCallback, useEffect, useMemo, useState } from 'react'
import { Home, KeyRound, PlayCircle, RefreshCw, ShieldCheck, Trash2, UserPlus } from 'lucide-react'
import {
  api,
  ApiClientError,
  createIdempotencyKey,
  type HouseholdConsent,
  type HouseholdGrant,
  type HouseholdMembership,
} from '@/api/client'
import type { DurableJob, DurableResourceCapability, DurableResourceRef } from '@/api/types'
import { useAuthStore } from '@/store/auth'
import { ADMIN_ROLES, roleAllowed } from '@/auth/roles'

/**
 * The only canonical action a household `operate` grant can authorize (see
 * `GRANT_SCOPED_ACTIONS` in `backend/src/action_registry.rs`). Everything else keeps its
 * operator/admin role, so this page never offers a control it cannot complete.
 */
export const GRANT_SCOPED_ACTION = 'automation.run'

const CONSENT_PURPOSES = ['privacy_export', 'ai_assistance', 'cloud_export', 'support'] as const
const CONSENT_DOMAINS = ['identity', 'inventory', 'telemetry', 'configuration'] as const
const MEMBER_ROLES = ['member', 'guest', 'owner', 'admin'] as const
const PERMISSION_RANK: Record<string, number> = { read: 1, operate: 2, admin: 3 }

export const RESOURCE_WINDOW = 50

/** Multiplier-free ranking of the household grant vocabulary; unknown values rank 0. */
function permissionRank(permission: string | undefined): number {
  return permission ? (PERMISSION_RANK[permission] ?? 0) : 0
}
const GRANT_PERMISSIONS = ['read', 'operate', 'admin'] as const

function shortId(value: string): string {
  return value.length > 12 ? `${value.slice(0, 8)}…` : value
}

function describeError(failure: unknown, fallback: string): string {
  if (failure instanceof ApiClientError) {
    return failure.code ? `${failure.message} (${failure.code})` : failure.message
  }
  return fallback
}

function Card({ title, children }: { title: string; children: React.ReactNode }) {
  return (
    <section
      className="rounded-lg p-4"
      style={{ background: 'var(--bg-surface)', border: '1px solid var(--border-subtle)' }}
      aria-label={title}
    >
      <h2 className="text-sm font-semibold" style={{ color: 'var(--text-primary)' }}>
        {title}
      </h2>
      <div className="mt-3 space-y-3 text-sm" style={{ color: 'var(--text-secondary)' }}>
        {children}
      </div>
    </section>
  )
}

const inputClass = 'rounded px-2 py-1 text-sm'
const inputStyle = {
  background: 'var(--bg-base)',
  border: '1px solid var(--border-subtle)',
  color: 'var(--text-primary)',
} as const

const buttonStyle = {
  background: 'var(--bg-base)',
  border: '1px solid var(--border-subtle)',
  color: 'var(--text-secondary)',
} as const

export default function HouseholdsPage() {
  const role = useAuthStore((state) => state.user?.role)
  const userId = useAuthStore((state) => state.user?.id)

  const [memberships, setMemberships] = useState<HouseholdMembership[]>([])
  const [selectedId, setSelectedId] = useState<string>('')
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState<string | null>(null)

  const [newHouseholdName, setNewHouseholdName] = useState('')
  const [newMemberId, setNewMemberId] = useState('')
  const [newMemberRole, setNewMemberRole] = useState<string>('member')
  const [grantUser, setGrantUser] = useState('')
  const [grantResourceId, setGrantResourceId] = useState('')
  const [grantPermission, setGrantPermission] = useState<string>('read')
  const [consentPurpose, setConsentPurpose] = useState<string>('privacy_export')
  const [consentDomain, setConsentDomain] = useState<string>('inventory')

  const [members, setMembers] = useState<HouseholdMembership[]>([])
  const [grants, setGrants] = useState<HouseholdGrant[]>([])
  const [consents, setConsents] = useState<HouseholdConsent[]>([])
  const [manageError, setManageError] = useState<string | null>(null)
  const [notice, setNotice] = useState<string | null>(null)
  const [busy, setBusy] = useState(false)

  const [accessible, setAccessible] = useState<DurableResourceRef[]>([])
  const [myPermissions, setMyPermissions] = useState<Record<string, string>>({})
  const [planView, setPlanView] = useState<{ risk: string; policy: string } | null>(null)
  const [consentsTruncated, setConsentsTruncated] = useState(false)
  const [capabilities, setCapabilities] = useState<Record<string, DurableResourceCapability[]>>({})
  const [accessError, setAccessError] = useState<string | null>(null)

  const [job, setJob] = useState<DurableJob | null>(null)
  const [runError, setRunError] = useState<string | null>(null)
  const [runningResource, setRunningResource] = useState<string | null>(null)

  const visibleResources = useMemo(
    () => accessible.slice(0, RESOURCE_WINDOW),
    [accessible],
  )

  const selected = useMemo(
    () => memberships.find((membership) => membership.household_id === selectedId) ?? null,
    [memberships, selectedId],
  )

  const canManageSelected = useMemo(() => {
    if (!selected) return false
    if (roleAllowed(role, ADMIN_ROLES)) return true
    return selected.role === 'owner' || selected.role === 'admin'
  }, [role, selected])

  const loadHouseholds = useCallback(async () => {
    setLoading(true)
    setError(null)
    try {
      const [mine, resources] = await Promise.all([api.households.list(), api.resources.list(200)])
      setMemberships(mine.memberships)
      setAccessible(resources.resources)
      setSelectedId((current) => {
        if (current && mine.memberships.some((row) => row.household_id === current)) return current
        return mine.memberships[0]?.household_id ?? ''
      })
    } catch (failure) {
      setMemberships([])
      setAccessible([])
      setError(describeError(failure, 'Households could not be loaded'))
    } finally {
      setLoading(false)
    }
  }, [])

  useEffect(() => { void loadHouseholds() }, [loadHouseholds])

  const loadAccess = useCallback(async (householdId: string) => {
    setAccessError(null)
    try {
      const [resources, mine] = await Promise.all([
        api.resources.list(200),
        api.households.myGrants(householdId),
      ])
      setAccessible(resources.resources)
      setMyPermissions(
        Object.fromEntries(mine.grants.map((grant) => [grant.resource_id, grant.permission])),
      )
      // Capabilities are only needed for the one grant-scoped action, so the page reads
      // detail for automation resources only instead of the whole resource catalogue.
      const automation = resources.resources.filter((resource) => resource.kind === 'automation_job')
      const details = await Promise.all(
        automation.map(async (resource) => {
          try {
            const read = await api.resources.get(resource.id)
            return [resource.id, read.capabilities] as const
          } catch {
            return [resource.id, [] as DurableResourceCapability[]] as const
          }
        }),
      )
      setCapabilities(Object.fromEntries(details))
    } catch (failure) {
      setAccessError(describeError(failure, 'Household resources could not be loaded'))
    }
  }, [])

  const loadManagement = useCallback(async (householdId: string) => {
    if (!householdId) return
    setManageError(null)
    try {
      const [memberList, grantList, consentList] = await Promise.all([
        api.households.members(householdId),
        api.households.grants(householdId),
        api.households.consents(householdId),
      ])
      setMembers(memberList.memberships)
      setGrants(grantList.grants.filter((grant) => grant.revoked_at === null))
      setConsents(consentList.consents)
      setConsentsTruncated(consentList.has_more)
    } catch (failure) {
      setMembers([])
      setGrants([])
      setConsents([])
      setManageError(describeError(failure, 'Household administration could not be loaded'))
    }
  }, [])

  useEffect(() => {
    if (!selectedId) {
      setMembers([])
      setGrants([])
      setConsents([])
      return
    }
    void loadAccess(selectedId)
    if (canManageSelected) void loadManagement(selectedId)
  }, [selectedId, canManageSelected, loadAccess, loadManagement])

  const withBusy = async (work: () => Promise<void>) => {
    setBusy(true)
    setManageError(null)
    setNotice(null)
    try {
      await work()
    } catch (failure) {
      setManageError(describeError(failure, 'The household change was refused'))
    } finally {
      setBusy(false)
    }
  }

  const createHousehold = () =>
    withBusy(async () => {
      const created = await api.households.create(newHouseholdName.trim())
      setNewHouseholdName('')
      await loadHouseholds()
      setSelectedId(created.household.id)
    })

  const addMember = () =>
    withBusy(async () => {
      await api.households.addMember(selectedId, {
        user_id: newMemberId.trim(),
        role: newMemberRole,
      })
      setNewMemberId('')
      await loadManagement(selectedId)
    })

  const revokeMember = (memberId: string) =>
    withBusy(async () => {
      await api.households.revokeMember(selectedId, memberId)
      await loadManagement(selectedId)
    })

  const submitGrant = () =>
    withBusy(async () => {
      await api.households.grant(selectedId, {
        user_id: grantUser.trim(),
        resource_id: grantResourceId.trim(),
        permission: grantPermission,
      })
      setGrantResourceId('')
      await loadManagement(selectedId)
    })

  const revokeGrant = (grant: HouseholdGrant) =>
    withBusy(async () => {
      await api.households.revokeGrant(selectedId, grant.resource_id, grant.user_id)
      await loadManagement(selectedId)
    })

  const createConsent = () =>
    withBusy(async () => {
      if (!userId) return
      await api.households.createConsent(selectedId, {
        user_id: userId,
        purpose: consentPurpose,
        domain: consentDomain,
        destination: 'local',
      })
      setNotice(`Consent recorded for ${consentDomain} (${consentPurpose}).`)
      if (canManageSelected) await loadManagement(selectedId)
    })

  const withdrawConsent = (consent: HouseholdConsent) =>
    withBusy(async () => {
      await api.households.withdrawConsent(selectedId, consent.id)
      await loadManagement(selectedId)
    })

  const runAutomation = async (resource: DurableResourceRef) => {
    setRunningResource(resource.id)
    setRunError(null)
    setPlanView(null)
    setJob(null)
    try {
      // Plan first: the server-derived plan, its risk, and the policy verdict are rendered
      // before submit is called, so the request is not a blind mutation.
      const planned = await api.canonicalActions.plan(resource.id, GRANT_SCOPED_ACTION, {})
      setPlanView({
        risk: planned.plan.operation.risk,
        policy: planned.plan.policy.outcome,
      })
      const submitted = await api.canonicalActions.submit(
        resource.id,
        GRANT_SCOPED_ACTION,
        {},
        createIdempotencyKey(),
      )
      // `submit` parses the canonical job envelope; the summary is the durable job read model.
      setJob(submitted.job as DurableJob)
    } catch (failure) {
      setRunError(describeError(failure, 'The household action could not be requested'))
    } finally {
      setRunningResource(null)
    }
  }

  const automationResources = accessible.filter((resource) => resource.kind === 'automation_job')

  return (
    <div className="mx-auto max-w-6xl space-y-5">
      <header className="flex flex-wrap items-end justify-between gap-4">
        <div>
          <div
            className="text-[10px] font-semibold uppercase tracking-[0.16em]"
            style={{ color: 'var(--accent-primary)' }}
            aria-hidden="true"
          >
            Household access
          </div>
          <h1 className="mt-1 text-2xl font-semibold" style={{ color: 'var(--text-primary)' }}>
            Households
          </h1>
          <p className="mt-1 max-w-2xl text-sm" style={{ color: 'var(--text-secondary)' }}>
            A household grant names one canonical resource UUID and one permission. Access is
            additive: it never creates a second identity or a second resource. Members only reach
            resources an owner granted to them, and every request still runs through the canonical
            plan, policy, job, audit, and event boundary.
          </p>
        </div>
        <button
          onClick={() => void loadHouseholds()}
          className="inline-flex items-center gap-1 rounded px-3 py-1.5 text-xs"
          style={buttonStyle}
          disabled={loading}
        >
          <RefreshCw size={13} /> Reload
        </button>
      </header>

      {loading && (
        <p role="status" className="text-sm" style={{ color: 'var(--text-secondary)' }}>
          Loading households…
        </p>
      )}

      {error && (
        <div
          role="alert"
          className="rounded-lg p-4 text-sm"
          style={{ background: 'var(--bg-surface)', border: '1px solid var(--border-subtle)', color: 'var(--text-primary)' }}
        >
          <p>{error}</p>
          <button
            onClick={() => void loadHouseholds()}
            className="mt-2 inline-flex items-center gap-1 rounded px-2 py-1 text-xs"
            style={buttonStyle}
          >
            <RefreshCw size={12} /> Retry
          </button>
        </div>
      )}

      {!loading && !error && memberships.length === 0 && (
        <Card title="No household yet">
          <p>You are not an active member of a household. An owner or administrator can create one below.</p>
        </Card>
      )}

      {!loading && !error && memberships.length > 0 && (
        <Card title="Your households">
          <label className="flex flex-wrap items-center gap-2">
            <span className="text-xs uppercase tracking-[0.08em]">Household</span>
            <select
              aria-label="Household"
              className={inputClass}
              style={inputStyle}
              value={selectedId}
              onChange={(event) => setSelectedId(event.target.value)}
            >
              {memberships.map((membership) => (
                <option key={membership.household_id} value={membership.household_id}>
                  {membership.household_id} · {membership.role}
                </option>
              ))}
            </select>
          </label>
          <ul className="space-y-1">
            {memberships.map((membership) => (
              <li key={`${membership.household_id}-${membership.role}`} className="flex items-center gap-2">
                <Home size={13} aria-hidden="true" />
                <span style={{ color: 'var(--text-primary)' }}>{shortId(membership.household_id)}</span>
                <span>household role: {membership.role}</span>
                <span>status: {membership.status}</span>
              </li>
            ))}
          </ul>
        </Card>
      )}

      {roleAllowed(role, ADMIN_ROLES) && (
        <Card title="Create a household">
          <div className="flex flex-wrap items-end gap-2">
            <label className="flex flex-col gap-1">
              <span className="text-xs uppercase tracking-[0.08em]">Name</span>
              <input
                aria-label="Household name"
                className={inputClass}
                style={inputStyle}
                value={newHouseholdName}
                onChange={(event) => setNewHouseholdName(event.target.value)}
                maxLength={128}
              />
            </label>
            <button
              onClick={() => void createHousehold()}
              disabled={busy || newHouseholdName.trim().length === 0}
              className="inline-flex items-center gap-1 rounded px-3 py-1.5 text-xs disabled:opacity-40"
              style={buttonStyle}
            >
              <Home size={13} /> Create
            </button>
          </div>
        </Card>
      )}

      {canManageSelected && (
        <>
          <Card title="Members">
            {manageError && <p role="alert">{manageError}</p>}
            {members.length === 0 ? (
              <p>No members yet.</p>
            ) : (
              <table className="w-full text-left text-xs">
                <thead>
                  <tr style={{ color: 'var(--text-secondary)' }}>
                    <th scope="col" className="py-1">Member</th>
                    <th scope="col" className="py-1">Household role</th>
                    <th scope="col" className="py-1">Status</th>
                    <th scope="col" className="py-1">Revoke</th>
                  </tr>
                </thead>
                <tbody>
                  {members.map((member) => (
                    <tr key={member.user_id}>
                      <td className="py-1" style={{ color: 'var(--text-primary)' }}>{member.user_id}</td>
                      <td className="py-1">{member.role}</td>
                      <td className="py-1">{member.status}</td>
                      <td className="py-1">
                        <button
                          onClick={() => void revokeMember(member.user_id)}
                          disabled={busy || member.user_id === userId}
                          aria-label={`Revoke membership ${member.user_id}`}
                          className="inline-flex items-center gap-1 rounded px-2 py-0.5 text-[11px] disabled:opacity-40"
                          style={buttonStyle}
                        >
                          <Trash2 size={11} /> Revoke
                        </button>
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            )}
            <p className="text-xs">
              Revoking a membership also ends that user's sessions, removes their API tokens, and
              cancels their open jobs across the instance.
            </p>
            <div className="flex flex-wrap items-end gap-2">
              <label className="flex flex-col gap-1">
                <span className="text-xs uppercase tracking-[0.08em]">User ID</span>
                <input
                  aria-label="Member user ID"
                  className={inputClass}
                  style={inputStyle}
                  value={newMemberId}
                  onChange={(event) => setNewMemberId(event.target.value)}
                />
              </label>
              <label className="flex flex-col gap-1">
                <span className="text-xs uppercase tracking-[0.08em]">Household role</span>
                <select
                  aria-label="Member household role"
                  className={inputClass}
                  style={inputStyle}
                  value={newMemberRole}
                  onChange={(event) => setNewMemberRole(event.target.value)}
                >
                  {MEMBER_ROLES.map((value) => (
                    <option key={value} value={value}>{value}</option>
                  ))}
                </select>
              </label>
              <button
                onClick={() => void addMember()}
                disabled={busy || newMemberId.trim().length === 0}
                className="inline-flex items-center gap-1 rounded px-3 py-1.5 text-xs disabled:opacity-40"
                style={buttonStyle}
              >
                <UserPlus size={13} /> Add member
              </button>
            </div>
          </Card>

          <Card title="Resource grants">
            {grants.length === 0 ? (
              <p>No active resource grants.</p>
            ) : (
              <table className="w-full text-left text-xs">
                <thead>
                  <tr style={{ color: 'var(--text-secondary)' }}>
                    <th scope="col" className="py-1">Member</th>
                    <th scope="col" className="py-1">Canonical resource</th>
                    <th scope="col" className="py-1">Permission</th>
                    <th scope="col" className="py-1">Revoke</th>
                  </tr>
                </thead>
                <tbody>
                  {grants.map((grant) => (
                    <tr key={`${grant.resource_id}-${grant.user_id}`}>
                      <td className="py-1" style={{ color: 'var(--text-primary)' }}>{grant.user_id}</td>
                      <td className="py-1">{shortId(grant.resource_id)}</td>
                      <td className="py-1">{grant.permission}</td>
                      <td className="py-1">
                        <button
                          onClick={() => void revokeGrant(grant)}
                          disabled={busy}
                          aria-label={`Revoke grant ${grant.resource_id} for ${grant.user_id}`}
                          className="inline-flex items-center gap-1 rounded px-2 py-0.5 text-[11px] disabled:opacity-40"
                          style={buttonStyle}
                        >
                          <Trash2 size={11} /> Revoke
                        </button>
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            )}
            <div className="flex flex-wrap items-end gap-2">
              <label className="flex flex-col gap-1">
                <span className="text-xs uppercase tracking-[0.08em]">Member</span>
                <input
                  aria-label="Grant member user ID"
                  className={inputClass}
                  style={inputStyle}
                  value={grantUser}
                  onChange={(event) => setGrantUser(event.target.value)}
                />
              </label>
              <label className="flex flex-col gap-1">
                <span className="text-xs uppercase tracking-[0.08em]">Canonical resource UUID</span>
                <input
                  aria-label="Grant canonical resource UUID"
                  className={inputClass}
                  style={inputStyle}
                  value={grantResourceId}
                  onChange={(event) => setGrantResourceId(event.target.value)}
                  placeholder="resources.id"
                />
              </label>
              <label className="flex flex-col gap-1">
                <span className="text-xs uppercase tracking-[0.08em]">Permission</span>
                <select
                  aria-label="Grant permission"
                  className={inputClass}
                  style={inputStyle}
                  value={grantPermission}
                  onChange={(event) => setGrantPermission(event.target.value)}
                >
                  {GRANT_PERMISSIONS.map((value) => (
                    <option key={value} value={value}>{value}</option>
                  ))}
                </select>
              </label>
              <button
                onClick={() => void submitGrant()}
                disabled={busy || grantUser.trim().length === 0 || grantResourceId.trim().length === 0}
                className="inline-flex items-center gap-1 rounded px-3 py-1.5 text-xs disabled:opacity-40"
                style={buttonStyle}
              >
                <ShieldCheck size={13} /> Grant
              </button>
            </div>
          </Card>

          <Card title="Consent">
            {consentsTruncated && (
              <p className="text-xs">More consent records exist than this page shows.</p>
            )}
            {consents.length === 0 ? (
              <p>No consent records for this household.</p>
            ) : (
              <ul className="space-y-1 text-xs">
                {consents.map((consent) => (
                  <li key={consent.id} className="flex flex-wrap items-center gap-2">
                    <span style={{ color: 'var(--text-primary)' }}>{consent.domain}</span>
                    <span>{consent.purpose}</span>
                    <span>status: {consent.status}</span>
                    <span>destination: {consent.destination}</span>
                    {consent.status === 'active' && (
                      <button
                        onClick={() => void withdrawConsent(consent)}
                        disabled={busy}
                        aria-label={`Withdraw consent ${consent.id}`}
                        className="inline-flex items-center gap-1 rounded px-2 py-0.5 text-[11px] disabled:opacity-40"
                        style={buttonStyle}
                      >
                        <Trash2 size={11} /> Withdraw
                      </button>
                    )}
                  </li>
                ))}
              </ul>
            )}
          </Card>
        </>
      )}

      {selected && (
        <Card title="Actions your grant allows">
          {notice && <p role="status">{notice}</p>}
          {accessError && <p role="alert">{accessError}</p>}
          {accessible.length === 0 && !accessError ? (
            <p>No canonical resource is visible to your household grants yet.</p>
          ) : (
            <ul className="space-y-2">
              {visibleResources.map((resource) => {
                const permission = myPermissions[resource.id]
                // `operate` is the permission the canonical grant check requires. Showing the
                // control for a read-only grant would offer something the server refuses.
                const runnable =
                  resource.kind === 'automation_job' &&
                  permissionRank(permission) >= permissionRank('operate') &&
                  (capabilities[resource.id] ?? []).some(
                    (capability) =>
                      capability.action === GRANT_SCOPED_ACTION && capability.availability === 'available',
                  )
                return (
                  <li key={resource.id} className="flex flex-wrap items-center gap-2">
                    <span style={{ color: 'var(--text-primary)' }}>{resource.display_name}</span>
                    <span className="text-xs">{resource.kind}</span>
                    <span className="text-xs">{shortId(resource.id)}</span>
                    <span className="text-xs">grant: {permission ?? 'none'}</span>
                    {runnable && (
                      <button
                        onClick={() => void runAutomation(resource)}
                        disabled={runningResource !== null}
                        aria-label={`Run ${resource.display_name}`}
                        className="inline-flex items-center gap-1 rounded px-2 py-0.5 text-[11px] disabled:opacity-40"
                        style={buttonStyle}
                      >
                        <PlayCircle size={11} /> {runningResource === resource.id ? 'Requesting…' : 'Run'}
                      </button>
                    )}
                    {resource.kind === 'automation_job' && permission && permissionRank(permission) < permissionRank('operate') && (
                      <span className="text-xs">read-only grant: the action requires operate</span>
                    )}
                  </li>
                )
              })}
            </ul>
          )}
          {accessible.length > RESOURCE_WINDOW && (
            <p className="text-xs">
              Showing {RESOURCE_WINDOW} of {accessible.length} visible resources.
            </p>
          )}
          {automationResources.length === 0 && !accessError && (
            <p className="text-xs">No automation resource is granted to this household.</p>
          )}
          {planView && (
            <p role="status" className="text-xs">
              Plan ready: risk {planView.risk}, policy {planView.policy}
            </p>
          )}
          {job && (
            <p role="status" className="text-xs">
              Job {job.id} · {job.action} · {job.state}
            </p>
          )}
          {runError && (
            <p role="alert" className="text-xs">
              {runError}
            </p>
          )}
        </Card>
      )}

      {selected && (
        <Card title="Record your consent">
          <div className="flex flex-wrap items-end gap-2">
            <label className="flex flex-col gap-1">
              <span className="text-xs uppercase tracking-[0.08em]">Purpose</span>
              <select
                aria-label="Consent purpose"
                className={inputClass}
                style={inputStyle}
                value={consentPurpose}
                onChange={(event) => setConsentPurpose(event.target.value)}
              >
                {CONSENT_PURPOSES.map((value) => (
                  <option key={value} value={value}>{value}</option>
                ))}
              </select>
            </label>
            <label className="flex flex-col gap-1">
              <span className="text-xs uppercase tracking-[0.08em]">Domain</span>
              <select
                aria-label="Consent domain"
                className={inputClass}
                style={inputStyle}
                value={consentDomain}
                onChange={(event) => setConsentDomain(event.target.value)}
              >
                {CONSENT_DOMAINS.map((value) => (
                  <option key={value} value={value}>{value}</option>
                ))}
              </select>
            </label>
            <button
              onClick={() => void createConsent()}
              disabled={busy}
              className="inline-flex items-center gap-1 rounded px-3 py-1.5 text-xs disabled:opacity-40"
              style={buttonStyle}
            >
              <KeyRound size={13} /> Record consent
            </button>
          </div>
          <p className="text-xs">Cloud export stays off: this records a local-only consent.</p>
        </Card>
      )}
    </div>
  )
}
