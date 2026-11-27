// The reader and solver, compiled to WebAssembly. Bytes in, JSON out (see crates/breach-wasm).
import type { Fate, Looked, Matcher, PlanOptions, Priority, Reading, Response, Route, RuleCheck } from './types'

interface Exports {
  memory: WebAssembly.Memory
  bp_alloc(len: number): number
  bp_free(ptr: number, len: number): void
  bp_read(rgba: number, width: number, height: number, opts: number, optsLen: number): number
  bp_plan(reading: number, readingLen: number, opts: number, optsLen: number): number
  bp_check(reading: number, readingLen: number, opts: number, optsLen: number): number
  bp_follow(rgba: number, width: number, height: number, state: number, stateLen: number): number
}

let instance: Promise<Exports> | null = null

function load(): Promise<Exports> {
  instance ??= (async () => {
    const url = new URL('./breach_wasm.wasm', document.baseURI)
    const res = await fetch(url)
    if (!res.ok) throw new Error(`could not load the reader (${res.status})`)
    const { instance } = await WebAssembly.instantiate(await res.arrayBuffer(), {})
    return instance.exports as unknown as Exports
  })()
  return instance
}

const encoder = new TextEncoder()
const decoder = new TextDecoder()

function put(x: Exports, bytes: Uint8Array | Uint8ClampedArray): [number, number] {
  const ptr = x.bp_alloc(bytes.length)
  new Uint8Array(x.memory.buffer, ptr, bytes.length).set(bytes)
  return [ptr, bytes.length]
}

function take<T = Response>(x: Exports, out: number): T {
  const len = new DataView(x.memory.buffer).getUint32(out, true)
  const json = decoder.decode(new Uint8Array(x.memory.buffer, out + 4, len))
  x.bp_free(out, len + 4)
  return JSON.parse(json) as T
}

function cleanOptions(opts: PlanOptions): string {
  // the core rejects explicit nulls for non-optional fields; drop empty keys
  return JSON.stringify(Object.fromEntries(Object.entries(opts).filter(([, v]) => v !== undefined)))
}

/** Read a frame of RGBA pixels and plan a route. */
export async function readRgba(rgba: Uint8Array | Uint8ClampedArray, width: number, height: number, opts: PlanOptions = {}): Promise<Response> {
  const x = await load()
  const [p, len] = put(x, rgba)
  const [o, olen] = put(x, encoder.encode(cleanOptions(opts)))
  try {
    return take(x, x.bp_read(p, width, height, o, olen))
  } finally {
    x.bp_free(p, len)
    x.bp_free(o, olen)
  }
}

/** Plan again for a reading already made (buffer change, daemon toggles). */
export async function replan(reading: Reading, opts: PlanOptions): Promise<Response> {
  const x = await load()
  const [r, rlen] = put(x, encoder.encode(JSON.stringify(reading)))
  const [o, olen] = put(x, encoder.encode(cleanOptions(opts)))
  try {
    return take(x, x.bp_plan(r, rlen, o, olen))
  } finally {
    x.bp_free(r, rlen)
    x.bp_free(o, olen)
  }
}

/** The rule check a reading offers given what is already known about the rule, or null when it offers none. */
export async function findCheck(reading: Reading, opts: PlanOptions): Promise<RuleCheck | null> {
  const x = await load()
  const [r, rlen] = put(x, encoder.encode(JSON.stringify(reading)))
  const [o, olen] = put(x, encoder.encode(cleanOptions(opts)))
  try {
    const found = take<{ status: string; check?: RuleCheck }>(x, x.bp_check(r, rlen, o, olen))
    return found.status === 'ok' && found.check ? found.check : null
  } finally {
    x.bp_free(r, rlen)
    x.bp_free(o, olen)
  }
}

/** A breach being played, as the page keeps it between two looks. */
export interface Following {
  reading: Reading
  /** The route the page shows. */
  route: Route
  check: boolean
  wanted: boolean[] | null
  buffer: number | null
  played: number[]
  fates: Fate[]
  priority: Priority
  matcher: Matcher | null
}

/** Look at a later frame of a screen read earlier: the picks made since, the daemons settled, the route that now fits. */
export async function follow(rgba: Uint8Array | Uint8ClampedArray, width: number, height: number, state: Following): Promise<Looked | null> {
  const x = await load()
  const [p, len] = put(x, rgba)
  const [s, slen] = put(x, encoder.encode(JSON.stringify(state)))
  try {
    const looked = take<Looked & { status: string }>(x, x.bp_follow(p, width, height, s, slen))
    return looked.status === 'ok' ? looked : null
  } finally {
    x.bp_free(p, len)
    x.bp_free(s, slen)
  }
}

/** Decode an image blob to RGBA pixels with the browser's own decoders. */
export async function pixelsOf(blob: Blob): Promise<{ rgba: Uint8ClampedArray; width: number; height: number }> {
  const bitmap = await createImageBitmap(blob, { colorSpaceConversion: 'none', premultiplyAlpha: 'none' })
  const canvas = new OffscreenCanvas(bitmap.width, bitmap.height)
  const ctx = canvas.getContext('2d', { willReadFrequently: true })
  if (!ctx) throw new Error('no 2D canvas available')
  ctx.drawImage(bitmap, 0, 0)
  bitmap.close()
  const data = ctx.getImageData(0, 0, canvas.width, canvas.height)
  return { rgba: data.data, width: data.width, height: data.height }
}

/** Warm the module so the first real read does not pay for compilation. */
export function warm(): void {
  void load().catch(() => undefined)
}
