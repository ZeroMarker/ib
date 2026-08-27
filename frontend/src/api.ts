export class ApiError extends Error {
  status: number
  constructor(status: number, message: string) {
    super(message)
    this.status = status
  }
}

export const api = async <T,>(path: string, init?: RequestInit): Promise<T> => {
  const response = await fetch(`api/${path}`, { credentials: 'same-origin', ...init })
  const data = await response.json().catch(() => undefined)
  if (!response.ok) throw new ApiError(response.status, data?.error ?? '请求失败，请稍后重试。')
  return data as T
}

export const json = (body: unknown): RequestInit => ({
  method: 'POST',
  headers: { 'Content-Type': 'application/json' },
  body: JSON.stringify(body),
})
