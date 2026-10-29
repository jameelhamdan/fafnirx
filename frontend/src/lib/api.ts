export class ApiError extends Error {
  status: number
  body: unknown

  constructor(status: number, message: string, body?: unknown) {
    super(message)
    this.status = status
    this.body = body
  }
}

/** Fired when the server says the login has ended, so the app can show the login page. */
export const UNAUTHORIZED = 'fx-unauthorized'

async function request<T>(method: string, url: string, body?: unknown): Promise<T> {
  const res = await fetch(url, {
    method,
    credentials: 'same-origin',
    // Marks the request as coming from this app: a form on another site cannot set it.
    headers: { 'x-requested-with': 'fafnirx', ...(body === undefined ? {} : { 'content-type': 'application/json' }) },
    body: body === undefined ? undefined : JSON.stringify(body),
  })
  if (!res.ok) {
    let message = res.statusText
    let parsed: unknown
    try {
      parsed = await res.json()
      message = (parsed as { error?: string }).error ?? message
    } catch {
      /* the body was not JSON */
    }
    if (res.status === 401 && !url.endsWith('/api/login')) window.dispatchEvent(new Event(UNAUTHORIZED))
    throw new ApiError(res.status, message, parsed)
  }
  return res.json() as Promise<T>
}

export const api = {
  get: <T>(url: string) => request<T>('GET', url),
  post: <T>(url: string, body: unknown = {}) => request<T>('POST', url, body),
  put: <T>(url: string, body: unknown) => request<T>('PUT', url, body),
  patch: <T>(url: string, body: unknown) => request<T>('PATCH', url, body),
  delete: <T>(url: string) => request<T>('DELETE', url),
}

/** `?a=1&b=2` from the defined, non-empty values. */
export function query(params: Record<string, string | number | boolean | undefined | null>): string {
  const q = new URLSearchParams()
  for (const [k, v] of Object.entries(params)) {
    if (v !== undefined && v !== null && v !== '' && v !== false) q.set(k, v === true ? '1' : String(v))
  }
  const text = q.toString()
  return text ? `?${text}` : ''
}

export const downloadUrl = (device: string, path: string, opts: { version?: number; inline?: boolean } = {}) =>
  `/api/files/download${query({ device, path, version: opts.version, inline: opts.inline })}`

export const thumbUrl = (device: string, path: string, size = 256) =>
  `/api/files/thumb${query({ device, path, size })}`

export type MediaView = 'thumb' | 'preview' | 'original' | 'download'

/** A view of a library file by its id. The hash makes the link specific to one content, so browsers keep it for good. */
export const mediaUrl = (id: string, view: MediaView, hash?: string) =>
  `/api/media/${id}/${view}${query({ v: hash })}`

/** Starts a browser download of a zip of the selection (a plain form post, so the browser streams it). */
export function downloadZip(items: { device: string; path: string }[], name: string) {
  const form = document.createElement('form')
  form.method = 'POST'
  form.action = '/api/files/zip'
  for (const [k, v] of [['items', JSON.stringify(items)], ['name', name]]) {
    const input = document.createElement('input')
    input.type = 'hidden'
    input.name = k
    input.value = v
    form.appendChild(input)
  }
  document.body.appendChild(form)
  form.submit()
  form.remove()
}
