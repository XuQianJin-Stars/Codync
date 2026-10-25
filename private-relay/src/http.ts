/** HTTP registration and the WebSocket upgrade. The upgrade is accepted only after admission. */

import { timingSafeEqual } from 'node:crypto'
import { createServer, type IncomingMessage, type Server } from 'node:http'
import type { Duplex } from 'node:stream'
import { Hono } from 'hono'
import { getRequestListener } from '@hono/node-server'
import { WebSocketServer } from 'ws'
import { COMPUTER_ID, computerIdFor, fromB64url } from './codec.ts'
import { signedAuthority, verifyCodyncSig } from './sig.ts'
import type { RelayStore } from './store.ts'
import { Rooms, type Admit } from './room.ts'

const MAX_BODY = 64 * 1024
const MAX_COMPUTERS = 32
const REGISTER_WINDOW_MS = 60_000
const REGISTER_LIMIT = 10
const REJECT_LIMIT = 30
const PLATFORMS = ['macos', 'linux', 'windows'] as const

/** Equal-length compare. A length mismatch is a miss; the token is not a secret length. */
function sameToken(given: string, expected: string): boolean {
  const left = Buffer.from(given)
  const right = Buffer.from(expected)
  return left.length === right.length && timingSafeEqual(left, right)
}

function prune(attempts: number[], at: number): void {
  let first = attempts[0]
  while (first !== undefined && at - first >= REGISTER_WINDOW_MS) {
    attempts.shift()
    first = attempts[0]
  }
}

function jsonError(status: number, code: string, message?: string): Response {
  return Response.json({ error: { code, ...(message ? { message } : {}) } }, { status })
}

function reject(socket: Duplex, status: number, code: string): void {
  const body = JSON.stringify({ error: { code } })
  const reason =
    status === 400 ? 'Bad Request'
    : status === 401 ? 'Unauthorized'
    : status === 403 ? 'Forbidden'
    : status === 404 ? 'Not Found'
    : status === 426 ? 'Upgrade Required'
    : status === 429 ? 'Too Many Requests'
    : 'Error'
  socket.write(
    `HTTP/1.1 ${status} ${reason}\r\nContent-Type: application/json\r\nContent-Length: ${Buffer.byteLength(body)}\r\nConnection: close\r\n\r\n${body}`,
  )
  socket.destroy()
}

interface Body {
  [key: string]: unknown
}

function objectBody(raw: Uint8Array): Body | Response {
  if (raw.length > MAX_BODY) return jsonError(400, 'badRequest', 'Body too large')
  if (!raw.length) return {}
  let value: unknown
  try {
    value = JSON.parse(new TextDecoder().decode(raw))
  } catch {
    return jsonError(400, 'badRequest', 'Body is not JSON')
  }
  if (!value || typeof value !== 'object' || Array.isArray(value)) return jsonError(400, 'badRequest', 'Body must be an object')
  return value as Body
}

function isResponse(v: Body | Response): v is Response {
  return v instanceof Response
}

/** Reads at most `MAX_BODY` bytes, including a request with no `Content-Length`. */
async function limitedBody(request: Request): Promise<Uint8Array | Response> {
  const declared = request.headers.get('content-length')
  if (declared !== null && (!/^\d+$/.test(declared) || Number(declared) > MAX_BODY)) {
    await request.body?.cancel().catch(() => {
      // The client may already be gone. The size refusal still stands.
    })
    return jsonError(400, 'badRequest', 'Body too large')
  }
  const reader = request.body?.getReader()
  if (!reader) return new Uint8Array()
  const chunks: Uint8Array[] = []
  let total = 0
  try {
    for (;;) {
      const { done, value } = await reader.read()
      if (done) break
      if (!value?.byteLength) continue
      total += value.byteLength
      if (total > MAX_BODY) {
        await reader.cancel().catch(() => {
          // The client may already be gone. The size refusal still stands.
        })
        return jsonError(400, 'badRequest', 'Body too large')
      }
      chunks.push(value)
    }
  } catch {
    return jsonError(400, 'badRequest', 'Body is not JSON')
  }
  const raw = new Uint8Array(total)
  let offset = 0
  for (const chunk of chunks) {
    raw.set(chunk, offset)
    offset += chunk.byteLength
  }
  return raw
}

export interface RelayServer {
  server: Server
  url: string
  store: RelayStore
  rooms: Rooms
  close: () => Promise<void>
}

export async function startRelay(options: {
  store: RelayStore
  host?: string
  port?: number
  now?: () => number
  /** Required to enroll a computer this relay does not already know. */
  token?: string
  maxComputers?: number
  /** New enrollments allowed per minute. A known computer refreshing its row does not count. */
  newRegisterLimit?: number
  /** Wrong-token attempts allowed per minute. They do not consume `newRegisterLimit`. */
  rejectedRegisterLimit?: number
}): Promise<RelayServer> {
  const now = options.now ?? Date.now
  const rooms = new Rooms(options.store, now)
  const store = options.store
  const token = options.token ?? ''
  const maxComputers = options.maxComputers ?? MAX_COMPUTERS
  const newRegisterLimit = options.newRegisterLimit ?? REGISTER_LIMIT
  const rejectedRegisterLimit = options.rejectedRegisterLimit ?? REJECT_LIMIT
  const registerAttempts: number[] = []
  const rejectedAttempts: number[] = []
  const app = new Hono()

  app.get('/health', (c) => c.json({ ok: true }))

  app.post('/v1/host/register', async (c) => {
    const raw = await limitedBody(c.req.raw)
    if (raw instanceof Response) return raw
    const body = objectBody(raw)
    if (isResponse(body)) return body
    const signed = await requireSig(c.req.header('host'), c.req.header('codync-sig'), 'POST', new URL(c.req.url), raw, now, store)
    if (signed instanceof Response) return signed
    const box = fromB64url(body.boxKey, 32)
    const name = typeof body.name === 'string' ? body.name : ''
    const version = typeof body.version === 'string' ? body.version : ''
    const platform = body.platform
    const device = body.device
    if (!box || !name.trim() || name.length > 100 || !version.trim() || version.length > 32) return jsonError(400, 'badRequest', 'Invalid register')
    if (typeof platform !== 'string' || !PLATFORMS.includes(platform as (typeof PLATFORMS)[number])) return jsonError(400, 'badRequest', 'Invalid platform')
    if (device != null && (typeof device !== 'string' || device.length > 32)) return jsonError(400, 'badRequest', 'Invalid device')
    const id = await computerIdFor(fromB64url(signed.kid, 32)!)
    const existing = store.computer(id)
    if (existing && existing.status !== 'active') return jsonError(403, 'forbidden')
    // A key this relay already accepted can refresh. Any other key needs the operator's token,
    // and the table stops growing once it is full.
    if (!existing) {
      const at = now()
      const given = c.req.header('codync-relay-token') ?? ''
      // A wrong token must not use up the allowance a real computer needs to enroll.
      if (!token || !sameToken(given, token)) {
        prune(rejectedAttempts, at)
        if (rejectedAttempts.length >= rejectedRegisterLimit) return jsonError(429, 'rateLimited')
        rejectedAttempts.push(at)
        return jsonError(403, 'forbidden')
      }
      prune(registerAttempts, at)
      if (registerAttempts.length >= newRegisterLimit) return jsonError(429, 'rateLimited')
      registerAttempts.push(at)
      if (store.countComputers() >= maxComputers) return jsonError(403, 'forbidden')
    }
    store.register({
      id,
      signPub: signed.kid,
      boxPub: body.boxKey as string,
      name,
      platform,
      device: typeof device === 'string' ? device : null,
      version,
      now: now(),
    })
    return c.json({ computerId: id, owned: false })
  })

  app.get('/v1/host/state', async (c) => {
    const signed = await requireSig(c.req.header('host'), c.req.header('codync-sig'), 'GET', new URL(c.req.url), new Uint8Array(), now, store)
    if (signed instanceof Response) return signed
    const id = await computerIdFor(fromB64url(signed.kid, 32)!)
    const computer = store.computer(id)
    if (!computer || computer.status !== 'active') return jsonError(404, 'unknownComputer')
    return c.json({ owner: null, grants: [], requests: [] })
  })

  const missing = () => jsonError(404, 'notFound')
  app.post('/v1/host/screen-ice', missing)
  app.post('/v1/host/unclaim', missing)
  app.post('/v1/host/grants/:id/revoke', missing)
  app.all('/v1/host/access-requests/:id/:action', missing)
  app.all('/v1/hooks/:computerId/:hookId', missing)

  const listener = getRequestListener(app.fetch)
  const server = createServer((req, res) => {
    listener(req, res)
  })
  const wss = new WebSocketServer({ noServer: true, maxPayload: 1024 * 1024 })
  const sweep = setInterval(() => rooms.sweep(), 30_000)
  sweep.unref?.()

  server.on('upgrade', (req, socket, head) => {
    void upgrade(req, socket, head, wss, rooms, store, now).catch((error: unknown) => {
      console.error('upgrade failed', error)
      socket.destroy()
    })
  })

  const host = options.host ?? '127.0.0.1'
  const port = options.port ?? 0
  await new Promise<void>((resolve) => server.listen(port, host, resolve))
  const address = server.address()
  const bound = typeof address === 'object' && address ? address.port : port
  return {
    server,
    url: `http://${host}:${bound}`,
    store,
    rooms,
    close: () =>
      new Promise((resolve, reject) => {
        clearInterval(sweep)
        for (const client of wss.clients) client.terminate()
        wss.close()
        server.close((error) => (error ? reject(error) : resolve()))
      }),
  }
}

async function requireSig(
  host: string | undefined,
  header: string | undefined,
  method: string,
  url: URL,
  body: Uint8Array,
  now: () => number,
  store: RelayStore,
): Promise<{ kid: string; nonce: string } | Response> {
  if (!host) return jsonError(400, 'badRequest', 'Missing Host')
  const signed = await verifyCodyncSig(header, method, signedAuthority(host), url.pathname + url.search, body, now())
  if (!signed) return jsonError(401, 'badSignature')
  if (!store.rememberNonce(signed.kid, signed.nonce, now())) return jsonError(401, 'badSignature')
  return signed
}

async function upgrade(
  req: IncomingMessage,
  socket: Duplex,
  head: Buffer,
  wss: WebSocketServer,
  rooms: Rooms,
  store: RelayStore,
  now: () => number,
): Promise<void> {
  const url = new URL(req.url ?? '/', 'http://relay.local')
  if (url.searchParams.get('v') !== '1') return reject(socket, 426, 'upgradeRequired')
  const host = req.headers.host
  if (!host || req.method !== 'GET') return reject(socket, 400, 'badRequest')
  const header = req.headers['codync-sig']
  if (Array.isArray(header)) return reject(socket, 401, 'badSignature')
  const signed = await verifyCodyncSig(header, 'GET', signedAuthority(host), url.pathname + url.search, new Uint8Array(), now())
  if (!signed) return reject(socket, 401, 'badSignature')
  if (!store.rememberNonce(signed.kid, signed.nonce, now())) return reject(socket, 401, 'badSignature')

  if (url.pathname === '/v1/relay/host') {
    const id = await computerIdFor(fromB64url(signed.kid, 32)!)
    const admitted = rooms.admitHost(id, signed.kid)
    if (!admitted.ok) return reject(socket, admitted.status, admitted.code)
    wss.handleUpgrade(req, socket, head, (ws) => rooms.attachHost(id, signed.kid, ws))
    return
  }

  const device = url.pathname.match(/^\/v1\/relay\/device\/([^/]+)$/)
  const computerId = device?.[1]
  if (!computerId) return reject(socket, 404, 'notFound')
  if (!COMPUTER_ID.test(computerId)) return reject(socket, 400, 'badRequest')
  const pair = url.searchParams.get('pair') ?? ''
  if (pair && !fromB64url(pair, 16)) return reject(socket, 400, 'badRequest')
  const admitted = rooms.admitDevice(computerId, signed.kid, pair)
  if (!admitted.ok) return reject(socket, admitted.status, admitted.code)
  const accept: Admit = admitted
  wss.handleUpgrade(req, socket, head, (ws) => rooms.attachDevice(computerId, signed.kid, accept, ws))
}
