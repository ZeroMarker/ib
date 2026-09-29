import { afterEach, describe, expect, it, vi } from 'vitest'
import { ApiError, api, json } from '../api'

const stubFetch = (response: Partial<Response> & { json?: () => Promise<unknown> }) => {
  const mock = vi.fn().mockResolvedValue({
    ok: response.ok ?? true,
    status: response.status ?? 200,
    json: response.json ?? (() => Promise.resolve({})),
  })
  vi.stubGlobal('fetch', mock)
  return mock
}

afterEach(() => vi.unstubAllGlobals())

describe('api', () => {
  it('prefixes the path with api/ and sends cookies', async () => {
    const mock = stubFetch({ json: () => Promise.resolve({ ok: true }) })
    await api<{ ok: boolean }>('trading/overview')
    expect(mock).toHaveBeenCalledWith('api/trading/overview', {
      credentials: 'same-origin',
    })
  })

  it('returns the parsed body on success', async () => {
    stubFetch({ json: () => Promise.resolve({ email: 'a@b.com' }) })
    await expect(api<{ email: string }>('auth/me')).resolves.toEqual({ email: 'a@b.com' })
  })

  it('throws ApiError carrying the server message and status', async () => {
    stubFetch({
      ok: false,
      status: 403,
      json: () => Promise.resolve({ error: 'email not verified' }),
    })
    const failure = await api('auth/login').catch((reason: unknown) => reason)
    expect(failure).toBeInstanceOf(ApiError)
    expect((failure as ApiError).status).toBe(403)
    expect((failure as ApiError).message).toBe('email not verified')
  })

  it('falls back to a generic message when the body is not JSON', async () => {
    stubFetch({ ok: false, status: 500, json: () => Promise.reject(new Error('bad json')) })
    const failure = (await api('auth/me').catch((reason: unknown) => reason)) as ApiError
    expect(failure.status).toBe(500)
    expect(failure.message).toBe('请求失败，请稍后重试。')
  })

  it('preserves a caller-supplied init', async () => {
    const mock = stubFetch({ json: () => Promise.resolve({}) })
    await api('trading/orders/1/cancel', { method: 'POST' })
    expect(mock).toHaveBeenCalledWith('api/trading/orders/1/cancel', {
      credentials: 'same-origin',
      method: 'POST',
    })
  })
})

describe('json', () => {
  it('builds a POST with a JSON content type', () => {
    expect(json({ email: 'a@b.com' })).toEqual({
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: '{"email":"a@b.com"}',
    })
  })
})
