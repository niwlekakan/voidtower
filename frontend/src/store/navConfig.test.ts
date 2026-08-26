import { describe, expect, it } from 'vitest'
import { resolvedNavGroups } from './navConfig'

describe('navigation default convergence', () => {
  it('adds new operation destinations to an existing customized Ops group', () => {
    const groups = resolvedNavGroups([{ id: 'ops', label: 'My tools', itemIds: ['terminal'] }])
    const ops = groups.find(group => group.id === 'ops')
    expect(ops?.label).toBe('My tools')
    expect(ops?.itemIds).toEqual(expect.arrayContaining(['terminal', 'jobs', 'approvals']))
  })
})
