import { asMatcher, type FrameInfo, type Matcher, type Plan, type PlanOptions, type Priority, type Reading, type Rejection, type Route, type RuleCheck } from './types'

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
  /** What is known about the wrong-pick rule: one rule, a pair still in play, or null for nothing. */
  matcher: Matcher | null
  /** The rule check being played on this screen in place of the route. */
  check: RuleCheck | null
  /** Breaches are spent on checking the rule until it is settled or the player stops. */
  checking: boolean
  /** What the last rule check found, in words. */
  ruleNote: string
  view: 'sheet' | 'screen'
}

// small things worth keeping between sessions
export const remembered = {
  get matcher(): Matcher | null {
    return asMatcher(localStorage.getItem('breacher.matcher'))
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

export const state: State = { frame: null, reading: null, plan: null, rejection: null, wanted: [], buffer: null, priority: 'count', optimistic: false, matcher: remembered.matcher, check: null, checking: false, ruleNote: '', view: 'sheet' }

export const planOptions = (): PlanOptions => ({ buffer: state.buffer, wanted: state.wanted.length ? state.wanted : null, priority: state.priority, matcher: state.matcher })

export const shownRoute = (): Route | null => {
  if (!state.plan) return null
  if (state.check) return state.check.route
  return state.optimistic && state.plan.optimistic ? state.plan.optimistic : state.plan.route
}
