import type { DurableApproval, DurableJob } from '@/api/types'

export function durableJob(overrides: Partial<DurableJob> = {}): DurableJob {
  return {
    id: 'job-00000000-0000-0000-0000-000000000001',
    action: 'container.restart',
    resource: { id: 'resource-1', kind: 'container', display_name: 'web', revision: 3 },
    actor: { actor_type: 'human', id: 'user-1', source: 'http_session' },
    ingress: 'http',
    state: 'running',
    progress_current: 1,
    progress_total: 2,
    progress_message: 'Restarting container',
    plan: {
      schema_version: 1,
      title: 'Restart web',
      risk: 'mutate',
      changes: [{ label: 'Container', value: 'web' }],
      preview: null,
      external_fingerprint: 'fingerprint-1',
      steps: [{ kind: 'container.restart', name: 'Restart container', retry_class: 'safe', recovery_class: 'reconcile' }],
    },
    approval_id: null,
    result: null,
    error: null,
    submitted_at: 1_725_000_000,
    started_at: 1_725_000_001,
    finished_at: null,
    updated_at: 1_725_000_002,
    ...overrides,
  }
}

export function durableApproval(overrides: Partial<DurableApproval> = {}): DurableApproval {
  return {
    id: 'approval-00000000-0000-0000-0000-00000001',
    job_id: 'job-00000000-0000-0000-0000-000000000001',
    requirement: 'Administrator approval',
    reason: 'This action changes provider state.',
    status: 'pending',
    expires_at: 1_725_000_900,
    decided_by: null,
    decision_comment: null,
    requested_at: 1_725_000_000,
    decided_at: null,
    updated_at: 1_725_000_000,
    ...overrides,
  }
}
