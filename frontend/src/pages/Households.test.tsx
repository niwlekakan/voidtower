import { act, fireEvent, render, screen, waitFor } from '@testing-library/react'
import { MemoryRouter } from 'react-router-dom'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { api, ApiClientError, type HouseholdConsent, type HouseholdGrant, type HouseholdMembership } from '@/api/client'
import type { DurableJob, DurableResourceRef, Role } from '@/api/types'
import { useAuthStore } from '@/store/auth'
import HouseholdsPage, { GRANT_SCOPED_ACTION } from './Households'

const HOUSEHOLD_ID = 'hhhhhhhh-1111-4111-8111-111111111111'
const AUTOMATION_ID = 'rrrrrrrr-2222-4222-8222-222222222222'
const MEMBER_ID = 'member-1'

function signInAs(role: Role, id = 'user-1') {
  useAuthStore.setState({
    status: 'authenticated',
    user: {
      id,
      username: 'synthetic-user',
      role,
      force_password_change: false,
      totp_enabled: true,
      mfa_required: false,
    },
  })
}

const membership = (overrides: Partial<HouseholdMembership> = {}): HouseholdMembership => ({
  household_id: HOUSEHOLD_ID,
  user_id: MEMBER_ID,
  role: 'member',
  status: 'active',
  expires_at: null,
  revoked_at: null,
  created_at: 1_700_000_000,
  updated_at: 1_700_000_000,
  ...overrides,
})

const grant = (overrides: Partial<HouseholdGrant> = {}): HouseholdGrant => ({
  household_id: HOUSEHOLD_ID,
  resource_id: AUTOMATION_ID,
  user_id: MEMBER_ID,
  permission: 'operate',
  granted_by: 'owner-1',
  expires_at: null,
  revoked_at: null,
  created_at: 1_700_000_000,
  updated_at: 1_700_000_000,
  ...overrides,
})

const automation: DurableResourceRef = {
  id: AUTOMATION_ID,
  kind: 'automation_job',
  display_name: 'Evening scene',
  revision: 0,
}

const consent = (overrides: Partial<HouseholdConsent> = {}): HouseholdConsent => ({
  id: 'consent-1',
  schema_version: 1,
  household_id: HOUSEHOLD_ID,
  user_id: MEMBER_ID,
  purpose: 'privacy_export',
  domain: 'inventory',
  resource_id: null,
  destination: 'local',
  ai_cloud_export: false,
  status: 'active',
  expires_at: null,
  retention_until: null,
  withdrawn_at: null,
  created_by: MEMBER_ID,
  created_at: 1_700_000_000,
  updated_at: 1_700_000_000,
  ...overrides,
})

const job = (): DurableJob =>
  ({
    id: 'job-1',
    action: GRANT_SCOPED_ACTION,
    resource: automation,
    actor: { actor_type: 'human', id: MEMBER_ID, source: 'web' },
    ingress: 'http_session',
    state: 'queued',
    approval_id: null,
    progress_current: 0,
    progress_total: 1,
    progress_message: null,
    plan: {
      schema_version: 1,
      title: 'Run automation',
      risk: 'mutate',
      changes: [],
      preview: null,
      external_fingerprint: 'fingerprint',
      steps: [],
    },
    result: null,
    error: null,
    submitted_at: 1_700_000_000,
    updated_at: 1_700_000_000,
  }) as unknown as DurableJob

function mockMemberView(
  options: { capabilitiesAvailable?: boolean; permission?: string } = {},
) {
  const { capabilitiesAvailable = true, permission = 'operate' } = options
  vi.spyOn(api.households, 'list').mockResolvedValue({ memberships: [membership()] })
  vi.spyOn(api.households, 'myGrants').mockResolvedValue({
    grants: permission ? [grant({ permission })] : [],
  })
  vi.spyOn(api.resources, 'list').mockResolvedValue({ schema_version: 1, resources: [automation] })
  vi.spyOn(api.resources, 'get').mockResolvedValue({
    schema_version: 1,
    resource: automation,
    aliases: [],
    capabilities: [
      {
        resource_id: AUTOMATION_ID,
        action: GRANT_SCOPED_ACTION,
        availability: capabilitiesAvailable ? 'available' : 'unavailable',
        reason_code: null,
        detail: null,
        schema_version: 1,
        observed_at: 1_700_000_000,
      },
    ],
  })
}

function renderPage() {
  return render(
    <MemoryRouter initialEntries={['/households']}>
      <HouseholdsPage />
    </MemoryRouter>,
  )
}

describe('HouseholdsPage', () => {
  beforeEach(() => {
    vi.restoreAllMocks()
    vi.unstubAllGlobals()
    signInAs('member', MEMBER_ID)
  })

  it('a granted member sees the resource and runs it through plan then submit', async () => {
    mockMemberView()
    const planSpy = vi.spyOn(api.canonicalActions, 'plan').mockResolvedValue({
      schema_version: 1,
      resource_id: AUTOMATION_ID,
      action: GRANT_SCOPED_ACTION,
      plan: {
        action: GRANT_SCOPED_ACTION,
        resource: automation,
        input_schema_id: 'automation.run.input.v1',
        result_schema_id: 'automation.run.result.v1',
        operation: {
          schema_version: 1,
          title: 'Run automation',
          risk: 'mutate',
          changes: [],
          preview: null,
          external_fingerprint: 'fingerprint',
          steps: [],
        },
        policy: { outcome: 'allow', reason: null },
      },
    } as never)
    const submitSpy = vi
      .spyOn(api.canonicalActions, 'submit')
      .mockResolvedValue({ schema_version: 1, resource_id: AUTOMATION_ID, action: GRANT_SCOPED_ACTION, job: job() } as never)

    renderPage()

    expect(await screen.findByRole('heading', { level: 1, name: 'Households' })).toBeInTheDocument()
    const runButton = await screen.findByRole('button', { name: 'Run Evening scene' })
    expect(await screen.findByText('Evening scene')).toBeInTheDocument()

    await act(async () => {
      fireEvent.click(runButton)
    })

    await waitFor(() => expect(submitSpy).toHaveBeenCalledTimes(1))
    expect(planSpy).toHaveBeenCalledWith(AUTOMATION_ID, GRANT_SCOPED_ACTION, {})
    const [resourceArg, actionArg, inputArg, idempotencyKey] = submitSpy.mock.calls[0]
    expect(resourceArg).toBe(AUTOMATION_ID)
    expect(actionArg).toBe(GRANT_SCOPED_ACTION)
    expect(inputArg).toEqual({})
    expect(idempotencyKey).toMatch(/^[A-Za-z0-9][A-Za-z0-9._:-]{0,127}$/)

    expect(await screen.findByText(/Job job-1/)).toBeInTheDocument()
    expect(screen.getByText('Plan ready: risk mutate, policy allow')).toBeInTheDocument()
  })

  it('does not offer the action when the capability is unavailable', async () => {
    mockMemberView({ capabilitiesAvailable: false })
    renderPage()

    expect(await screen.findByText('Evening scene')).toBeInTheDocument()
    await waitFor(() => expect(api.resources.get).toHaveBeenCalled())
    expect(screen.queryByRole('button', { name: 'Run Evening scene' })).not.toBeInTheDocument()
  })

  it('offers the action only when the grant itself allows it', async () => {
    mockMemberView({ permission: 'read' })
    renderPage()

    expect(await screen.findByText('Evening scene')).toBeInTheDocument()
    expect(screen.queryByRole('button', { name: 'Run Evening scene' })).not.toBeInTheDocument()
    expect(screen.getByText('read-only grant: the action requires operate')).toBeInTheDocument()
    expect(screen.getByText('grant: read')).toBeInTheDocument()
  })

  it('reports a revoked grant as a bounded refusal and submits nothing', async () => {
    mockMemberView()
    vi.spyOn(api.canonicalActions, 'plan').mockRejectedValue(
      new ApiClientError('An explicit household grant is required for this resource.', 'resource_grant_required', 403),
    )
    const submitSpy = vi.spyOn(api.canonicalActions, 'submit')

    renderPage()
    const runButton = await screen.findByRole('button', { name: 'Run Evening scene' })
    await act(async () => {
      fireEvent.click(runButton)
    })

    expect(await screen.findByRole('alert')).toHaveTextContent(
      'An explicit household grant is required for this resource. (resource_grant_required)',
    )
    expect(submitSpy).not.toHaveBeenCalled()
  })

  it('shows loading, then a distinct empty state when no household exists', async () => {
    vi.spyOn(api.households, 'list').mockResolvedValue({ memberships: [] })
    vi.spyOn(api.resources, 'list').mockResolvedValue({ schema_version: 1, resources: [] })

    renderPage()

    expect(screen.getByRole('status')).toHaveTextContent('Loading households…')
    expect(await screen.findByText('No household yet')).toBeInTheDocument()
  })

  it('fails closed with a retry instead of rendering an empty household list', async () => {
    const listSpy = vi
      .spyOn(api.households, 'list')
      .mockRejectedValue(new ApiClientError('Household read failed.', 'internal', 500))
    vi.spyOn(api.resources, 'list').mockResolvedValue({ schema_version: 1, resources: [] })

    renderPage()

    expect(await screen.findByRole('alert')).toHaveTextContent('Household read failed. (internal)')
    expect(screen.queryByText('No household yet')).not.toBeInTheDocument()

    listSpy.mockResolvedValue({ memberships: [membership()] })
    await act(async () => {
      fireEvent.click(screen.getByRole('button', { name: /Retry/ }))
    })
    expect(await screen.findByRole('heading', { name: 'Your households' })).toBeInTheDocument()
  })

  it('lets a household manager grant and revoke a canonical resource', async () => {
    signInAs('owner', 'owner-1')
    vi.spyOn(api.households, 'list').mockResolvedValue({ memberships: [membership({ role: 'owner', user_id: 'owner-1' })] })
    vi.spyOn(api.households, 'myGrants').mockResolvedValue({ grants: [] })
    vi.spyOn(api.households, 'members').mockResolvedValue({
      memberships: [membership({ role: 'owner', user_id: 'owner-1' }), membership()],
    })
    const grantsSpy = vi.spyOn(api.households, 'grants').mockResolvedValue({ grants: [grant()] })
    vi.spyOn(api.households, 'consents').mockResolvedValue({
      schema_version: 1,
      consents: [consent()],
      next_cursor: null,
      has_more: false,
    })
    vi.spyOn(api.resources, 'list').mockResolvedValue({ schema_version: 1, resources: [automation] })
    vi.spyOn(api.resources, 'get').mockResolvedValue({
      schema_version: 1,
      resource: automation,
      aliases: [],
      capabilities: [],
    })
    const revokeGrant = vi.spyOn(api.households, 'revokeGrant').mockResolvedValue({ revoked: true })
    const grantSpy = vi
      .spyOn(api.households, 'grant')
      .mockResolvedValue({ household_id: HOUSEHOLD_ID, resource_id: AUTOMATION_ID, user_id: MEMBER_ID, permission: 'read' })
    const withdrawConsent = vi
      .spyOn(api.households, 'withdrawConsent')
      .mockResolvedValue({ schema_version: 1, withdrawn: true, consent_id: 'consent-1' })

    renderPage()

    expect(await screen.findByRole('heading', { name: 'Members' })).toBeInTheDocument()
    await waitFor(() => expect(grantsSpy).toHaveBeenCalledWith(HOUSEHOLD_ID))
    expect(
      await screen.findByRole('button', { name: `Revoke grant ${AUTOMATION_ID} for ${MEMBER_ID}` }),
    ).toBeInTheDocument()

    fireEvent.change(screen.getByLabelText('Grant member user ID'), { target: { value: 'member-2' } })
    fireEvent.change(screen.getByLabelText('Grant canonical resource UUID'), { target: { value: AUTOMATION_ID } })
    fireEvent.change(screen.getByLabelText('Grant permission'), { target: { value: 'read' } })
    await act(async () => {
      fireEvent.click(screen.getByRole('button', { name: /^Grant$/ }))
    })
    await waitFor(() =>
      expect(grantSpy).toHaveBeenCalledWith(HOUSEHOLD_ID, {
        user_id: 'member-2',
        resource_id: AUTOMATION_ID,
        permission: 'read',
      }),
    )

    await act(async () => {
      fireEvent.click(screen.getByRole('button', { name: `Revoke grant ${AUTOMATION_ID} for ${MEMBER_ID}` }))
    })
    await waitFor(() => expect(revokeGrant).toHaveBeenCalledWith(HOUSEHOLD_ID, AUTOMATION_ID, MEMBER_ID))

    await act(async () => {
      fireEvent.click(screen.getByRole('button', { name: 'Withdraw consent consent-1' }))
    })
    await waitFor(() => expect(withdrawConsent).toHaveBeenCalledWith(HOUSEHOLD_ID, 'consent-1'))
  })
})
