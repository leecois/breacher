// Mirrors the JSON that breach-core produces (api.rs, reader.rs, plan.rs).

export interface Geometry {
  x0: number
  y0: number
  px: number
  py: number
  n: number
  seq_x0: number
  seq_pitch_x: number
  seq_rows: number[]
  buffer_y: number
}

export interface Reading {
  n: number
  matrix: number[]
  sequences: number[][]
  buffer: number
  geometry: Geometry
  quality: { buffer_assumed?: boolean; layout_fallback?: boolean; [k: string]: unknown }
  timings: { total_us: number; [k: string]: number }
}

export interface Step {
  row: number
  col: number
  code: number
  x: number
  y: number
  uploads: number[]
}

export interface Route {
  steps: Step[]
  uploaded: number[]
  missed: number[]
  /** How many of the first steps have been played already. */
  done?: number
}

export interface Plan {
  buffer: number
  route: Route
  optimistic: Route | null
  proven: boolean
  solve_us: number
}

export type Stage = 'frame' | 'segment' | 'lattice' | 'in_progress' | 'sequences' | 'buffer' | 'confidence'

export type Response =
  | { status: 'ok'; reading: Reading; plan: Plan }
  | { status: 'rejected'; stage: Stage; message: string; hint: string }
  | { status: 'error'; message: string }

export type Rejection = Exclude<Response, { status: 'ok' }>

export type Priority = 'count' | 'tier'
export type Matcher = 'substring' | 'retest' | 'strict' | 'unanimous' | 'not_substring' | 'not_retest' | 'not_strict'

const KNOWN: readonly string[] = ['substring', 'retest', 'strict', 'not_substring', 'not_retest', 'not_strict']

/** What is known about the wrong-pick rule, from a stored value; null when nothing is. */
export const asMatcher = (v: unknown): Matcher | null => (typeof v === 'string' && KNOWN.includes(v) ? (v as Matcher) : null)

/** True once the rule is down to one. */
export const settled = (m: Matcher | null): boolean => m === 'substring' || m === 'retest' || m === 'strict'

/** One way a rule check can turn out. */
export interface Outcome {
  /** Daemons the game uploads in this case, by index. */
  uploaded: number[]
  /** What is known about the rule once this has been seen. */
  then: Matcher
}

/** A route to play once, chosen so that what uploads tells the wrong-pick rules apart. */
export interface RuleCheck {
  route: Route
  outcomes: Outcome[]
}

export interface PlanOptions {
  buffer?: number | null
  wanted?: boolean[] | null
  priority?: Priority
  matcher?: Matcher | null
  /** Reading only: buffer size to assume when the buffer panel cannot be read. */
  buffer_fallback?: number
}

/** What the game has made of a daemon. */
export type Fate = 'pending' | 'installed' | 'failed'

/** What one look at a breach being played found. */
export interface Looked {
  sight: 'breach' | 'other' | 'over'
  step: 'onward' | 'replanned' | null
  /** Daemons settled since the look before, counted from one. */
  settled: Array<[number, Fate]>
  /** The route as it now stands. */
  route: Route
  /** Cells picked so far, in order. */
  played: number[]
  fates: Fate[]
}

/** What a capture is pointed at: the game found on its own, a window by its title, or a display. */
export type Target = { kind: 'auto' } | { kind: 'window'; value: string } | { kind: 'display'; value: number }

export const CODES = ['1C', '55', '7A', 'BD', 'E9', 'FF'] as const

/** A frame as the UI knows it: where it came from and how big it was. */
export interface FrameInfo {
  width: number
  height: number
  source: string
  /** Object URL or data URL of the image, when one is available to show. */
  url?: string
  /** Wall-clock milliseconds from "have pixels" to "have a route". */
  elapsedMs?: number
}
