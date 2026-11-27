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

export type Priority = 'count' | 'tier'
export type Matcher = 'substring' | 'retest' | 'strict' | 'unanimous'

export interface PlanOptions {
  buffer?: number | null
  wanted?: boolean[] | null
  priority?: Priority
  matcher?: Matcher | null
  /** Reading only: buffer size to assume when the buffer panel cannot be read. */
  buffer_fallback?: number
}

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
