import type { FrameInfo, Matcher, Plan, PlanOptions, Priority, Reading, Rejection, Route } from './types'

export interface State {
  frame: FrameInfo | null
  reading: Reading | null
  plan: Plan | null
  /** Why the current frame was not read, when it was not. */
  rejection: Rejection | null
  /** Daemons the player wants, by index. */
  wanted: boolean[]
  /** Buffer size set by hand; null = what the screen shows. */
  buffer: number | null
  priority: Priority
  /** Show the route that depends on the unconfirmed overlap rule. */
  optimistic: boolean
  /** The wrong-pick rule, once the player has confirmed it in game. */
  matcher: Matcher | null
  view: 'sheet' | 'screen'
}

// small things worth keeping between sessions
export const remembered = {
  get matcher(): Matcher | null {
    const v = localStorage.getItem('breacher.matcher')
    return v === 'substring' || v === 'retest' || v === 'strict' ? v : null
  },
  set matcher(v: Matcher | null) {
    if (v) localStorage.setItem('breacher.matcher', v)
    else localStorage.removeItem('breacher.matcher')
  },
  /** Last buffer size read from a screen; assumed when a later screen's buffer panel is unreadable. */
  get buffer(): number {
    const v = Number(localStorage.getItem('breacher.buffer'))
    // 4 is the smallest buffer in the game: assuming too few slots can cost a daemon, never a breach
    return Number.isInteger(v) && v >= 2 && v <= 16 ? v : 4
  },
  set buffer(v: number) {
    localStorage.setItem('breacher.buffer', String(v))
  },
}

export const state: State = { frame: null, reading: null, plan: null, rejection: null, wanted: [], buffer: null, priority: 'count', optimistic: false, matcher: remembered.matcher, view: 'sheet' }

export const planOptions = (): PlanOptions => ({ buffer: state.buffer, wanted: state.wanted.length ? state.wanted : null, priority: state.priority, matcher: state.matcher })

export const shownRoute = (): Route | null => (state.plan ? (state.optimistic && state.plan.optimistic ? state.plan.optimistic : state.plan.route) : null)
