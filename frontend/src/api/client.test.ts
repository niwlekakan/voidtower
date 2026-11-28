import { afterEach, describe, expect, it, vi } from 'vitest'
import { createIdempotencyKey } from './client'

afterEach(() => vi.unstubAllGlobals())

describe('createIdempotencyKey', () => {
  it('creates a UUID v4 using getRandomValues without requiring randomUUID', () => {
    const getRandomValues = vi.fn((bytes: Uint8Array) => bytes.fill(0))
    vi.stubGlobal('crypto', { getRandomValues })

    expect(createIdempotencyKey()).toBe('00000000-0000-4000-8000-000000000000')
    expect(getRandomValues).toHaveBeenCalledTimes(1)
  })
})
