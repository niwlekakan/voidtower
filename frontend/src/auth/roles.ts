import type { Role } from '@/api/types'

export const OPERATOR_ROLES: readonly Role[] = ['owner', 'admin', 'operator']
export const ADMIN_ROLES: readonly Role[] = ['owner', 'admin']

export function roleAllowed(role: Role | undefined, allowed: readonly Role[]): boolean {
  return role !== undefined && allowed.includes(role)
}

export function operationNavItemAllowed(itemId: string, role: Role | undefined): boolean {
  if (itemId === 'jobs') return roleAllowed(role, OPERATOR_ROLES)
  if (itemId === 'approvals') return roleAllowed(role, ADMIN_ROLES)
  // CMDB reads are `Access::Read` on the server, which requires an operator role.
  if (itemId === 'inventory') return roleAllowed(role, OPERATOR_ROLES)
  // Household access is additive and every authenticated role has its own view; the
  // server still decides which household rows and resources each caller can see.
  return true
}
