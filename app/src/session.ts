import { findCheck, pixelsOf, readRgba, replan } from './core'
import { desktop } from './desktop'
import { $ } from './dom'
import { paintFloat } from './float'
import { render, renderRejected, ruleFinding, status, type Hooks } from './render'
import { planOptions, remembered, shownRoute, state } from './state'
import { settled, type FrameInfo, type Matcher, type Outcome, type Response, type Route, type RuleCheck } from './types'

const stage = $<HTMLElement>('stage')

function setStage(name: 'empty' | 'rejected' | 'result'): void {
  stage.dataset.state = name
  $<HTMLButtonElement>('home').hidden = name === 'empty'
}

const hooks: Hooks = {
  replan: () => void planAgain(),
  checking: on => void setChecking(on),
  outcome: seen => void answerCheck(seen),
}

export function redraw(animate: boolean): void {
  render(animate, hooks)
  paintFloat()
}

/** Sends what the window shows to the overlay over the game. */
function publish(response: Response, frame: FrameInfo): void {
  const route = shownRoute()
  desktop?.publish({ response, frame: { width: frame.width, height: frame.height, source: frame.source }, route, check: Boolean(state.check) })
  if (route) desktop?.follow(route, state.wanted.length ? state.wanted : null, state.buffer, Boolean(state.check))
}

/** Takes in a pick the shell saw in the game: the route counts it off, or gives way to the one planned from there. */
export function followed(route: Route, replanned: boolean): void {
  if (!state.plan || !state.reading) return
  if (replanned) {
    if (state.check) state.ruleNote = 'The rule check was not played as drawn, so it was dropped. The route now makes the most of where the breach stands.'
    state.check = null
    state.optimistic = false
    state.plan = { ...state.plan, route, optimistic: null }
  } else if (state.check) {
    state.check = { ...state.check, route }
  } else if (state.optimistic && state.plan.optimistic) {
    state.plan = { ...state.plan, optimistic: route }
  } else {
    state.plan = { ...state.plan, route }
  }
  redraw(false)
}

/** Reads a frame of pixels under the player's settings, showing nothing. */
export async function readPixels(rgba: Uint8Array | Uint8ClampedArray, width: number, height: number): Promise<{ response: Response; elapsedMs: number }> {
  const t = performance.now()
  const response = await readRgba(rgba, width, height, { priority: state.priority, matcher: state.matcher, buffer_fallback: remembered.buffer })
  return { response, elapsedMs: performance.now() - t }
}

/** Shows what a fresh read came to, with the rule check that stands in for the route when one is under way. */
export async function showRead(response: Response, frame: FrameInfo): Promise<void> {
  const check = response.status === 'ok' && state.checking ? await findCheck(response.reading, { matcher: state.matcher }) : null
  show(response, frame, true, check)
}

export async function readBlob(blob: Blob, source: string): Promise<void> {
  stage.dataset.busy = 'true'
  try {
    const url = URL.createObjectURL(blob)
    const { rgba, width, height } = await pixelsOf(blob)
    const { response, elapsedMs } = await readPixels(rgba, width, height)
    await showRead(response, { width, height, source, url, elapsedMs })
  } catch (e) {
    show({ status: 'error', message: e instanceof Error ? e.message : String(e) }, { width: 0, height: 0, source }, true)
  } finally {
    delete stage.dataset.busy
  }
}

/** Display a fresh answer. `fresh` resets the per-screen choices (buffer override, daemon toggles); `check` stands in for the route. */
export function show(response: Response, frame: FrameInfo, fresh: boolean, check: RuleCheck | null = null): void {
  if (state.frame?.url && state.frame.url !== frame.url) URL.revokeObjectURL(state.frame.url)
  state.frame = frame
  if (response.status === 'ok') {
    state.reading = response.reading
    state.plan = response.plan
    if (fresh) {
      state.wanted = response.reading.sequences.map(() => true)
      state.buffer = null
      state.optimistic = false
      state.check = state.checking ? check : null
      if (!response.reading.quality.buffer_assumed) remembered.buffer = response.reading.buffer
    }
    state.rejection = null
    setStage('result')
    redraw(fresh)
  } else {
    state.reading = null
    state.plan = null
    state.check = null
    state.rejection = response
    setStage('rejected')
    renderRejected(response, frame)
    paintFloat()
  }
  publish(response, frame)
}

/** Gives the current frame its picture once the desktop shell has encoded it. */
export function attachImage(frame: FrameInfo, url: string): void {
  if (state.frame !== frame) return
  frame.url = url
  if (state.reading) redraw(false)
  else if (state.rejection) renderRejected(state.rejection, frame)
}

/** Drops the current frame and returns to the start screen. */
export function home(): void {
  if (state.frame?.url) URL.revokeObjectURL(state.frame.url)
  state.frame = null
  state.reading = null
  state.plan = null
  state.check = null
  state.rejection = null
  setStage('empty')
  status([['Status', 'No frame yet']])
  desktop?.clearOverlay()
  paintFloat()
}

export async function planAgain(): Promise<void> {
  if (!state.reading || !state.frame) return
  const response = await replan(state.reading, planOptions())
  if (response.status === 'ok') {
    state.plan = response.plan
    if (!response.plan.optimistic) state.optimistic = false
    // a check depends on the buffer, so one that is under way is found again
    if (state.check) state.check = await findCheck(response.reading, planOptions())
    redraw(false)
    publish(response, state.frame)
  }
}

/** Starts or stops spending breaches on checking the wrong-pick rule; the screen in the window is used at once. */
export async function setChecking(on: boolean): Promise<void> {
  state.checking = on
  state.ruleNote = ''
  desktop?.setChecking(on)
  state.check = on && state.reading ? await findCheck(state.reading, planOptions()) : null
  if (state.reading && state.plan && state.frame) {
    redraw(false)
    publish({ status: 'ok', reading: state.reading, plan: state.plan }, state.frame)
  }
}

/** Takes in what a breach that was played showed about the wrong-pick rule, and plans with it from now on. */
export async function learnt(rule: Matcher): Promise<void> {
  state.check = null
  state.matcher = rule
  remembered.matcher = rule
  $<HTMLSelectElement>('matcher').value = rule
  state.ruleNote = `Seen in the breach you just played. ${ruleFinding(rule, state.checking ? undefined : 'Routes still work under both rules that remain.')}`
  if (settled(rule) && state.checking) {
    state.checking = false
    desktop?.setChecking(false)
  }
  await planAgain()
}

/** Takes in how a rule check turned out and plans again with what it showed. */
export async function answerCheck(seen: Outcome | 'other' | 'skipped'): Promise<void> {
  state.check = null
  const stop = (): void => {
    state.checking = false
    desktop?.setChecking(false)
  }
  if (seen === 'other') {
    state.ruleNote = 'None of the rules in play predicts that, so nothing was changed. Save a report and say what uploaded.'
    stop()
  } else if (seen !== 'skipped') {
    state.matcher = seen.then
    remembered.matcher = seen.then
    desktop?.setMatcher(seen.then)
    $<HTMLSelectElement>('matcher').value = seen.then
    state.ruleNote = ruleFinding(seen.then)
    if (settled(seen.then)) stop()
  }
  await planAgain()
}
