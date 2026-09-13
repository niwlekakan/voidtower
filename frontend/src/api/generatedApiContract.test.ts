import { describe, expect, it } from 'vitest'
import { API_V1_ENVELOPE_CONTRACT } from './generatedApiContract'

describe('generated API v1 envelope contract', () => {
  it('consumes the source-owned artifact through the generated client seam', () => {
    expect(API_V1_ENVELOPE_CONTRACT.contract).toBe('voidtower.api.envelopes')
    expect(API_V1_ENVELOPE_CONTRACT.api_version).toBe('1')
  })

  it('preserves the v1 action and error envelope seams', () => {
    expect(API_V1_ENVELOPE_CONTRACT.api_version).toBe('1')
    expect(API_V1_ENVELOPE_CONTRACT.envelopes.plan_success_v1).toMatchObject({
      schema_version: 1,
      resource_id: 'resource-1',
      action: 'container.start',
    })
    expect(API_V1_ENVELOPE_CONTRACT.envelopes.job_read_v1).toMatchObject({
      schema_version: 1,
      resource_id: 'resource-1',
      action: 'container.start',
    })
    expect(API_V1_ENVELOPE_CONTRACT.envelopes.error_v1.error).toEqual({
      code: 'job_not_found',
      message: 'The requested job does not exist.',
    })
  })
})
