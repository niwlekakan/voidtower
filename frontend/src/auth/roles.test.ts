import { describe, expect, it } from 'vitest'
import type { Role } from '@/api/types'
import { ADMIN_ROLES, operationNavItemAllowed, OPERATOR_ROLES, roleAllowed } from './roles'

const ROLES: Role[] = ['owner', 'admin', 'operator', 'viewer', 'guest', 'demo', 'member']

describe('durable operation role allowlists', () => {
  it('allows exactly owner, admin, and operator for jobs', () => {
    expect(ROLES.filter(role => roleAllowed(role, OPERATOR_ROLES))).toEqual(['owner', 'admin', 'operator'])
  })

  it('allows exactly owner and admin for approvals', () => {
    expect(ROLES.filter(role => roleAllowed(role, ADMIN_ROLES))).toEqual(['owner', 'admin'])
    expect(roleAllowed(undefined, ADMIN_ROLES)).toBe(false)
  })

  it('hides approval discovery from operators while retaining jobs', () => {
    expect(operationNavItemAllowed('jobs', 'operator')).toBe(true)
    expect(operationNavItemAllowed('approvals', 'operator')).toBe(false)
    expect(operationNavItemAllowed('approvals', 'admin')).toBe(true)
  })
})
