import { pixelsOf, readRgba, replan } from './core'
import { desktop } from './desktop'
import { $ } from './dom'
import { render, renderRejected, status } from './render'
import { planOptions, remembered, shownRoute, state } from './state'
import type { FrameInfo, Response } from './types'

const stage = $<HTMLElement>('stage')

function setStage(name: 'empty' | 'rejected' | 'result'): void {
  stage.dataset.state = name
  $<HTMLButtonElement>('home').hidden = name === 'empty'
}

export function redraw(animate: boolean): void {
  render(animate, () => void planAgain())
}

export async function readBlob(blob: Blob, source: string): Promise<void> {
  stage.dataset.busy = 'true'
  try {
    const url = URL.createObjectURL(blob)
    const { rgba, width, height } = await pixelsOf(blob)
    const t = performance.now()
    const response = await readRgba(rgba, width, height, { priority: state.priority, matcher: state.matcher, buffer_fallback: remembered.buffer })
    show(response, { width, height, source, url, elapsedMs: performance.now() - t }, true)
  } catch (e) {
    show({ status: 'error', message: e instanceof Error ? e.message : String(e) }, { width: 0, height: 0, source }, true)
  } finally {
    delete stage.dataset.busy
  }
}

/** Display a fresh answer. `fresh` resets the per-screen choices (buffer override, daemon toggles). */
export function show(response: Response, frame: FrameInfo, fresh: boolean): void {
  if (state.frame?.url && state.frame.url !== frame.url) URL.revokeObjectURL(state.frame.url)
  state.frame = frame
  if (response.status === 'ok') {
    state.reading = response.reading
    state.plan = response.plan
    if (fresh) {
      state.wanted = response.reading.sequences.map(() => true)
      state.buffer = null
      state.optimistic = false
      if (!response.reading.quality.buffer_assumed) remembered.buffer = response.reading.buffer
    }
    state.rejection = null
    setStage('result')
    redraw(fresh)
  } else {
    state.reading = null
    state.plan = null
    state.rejection = response
    setStage('rejected')
    renderRejected(response, frame)
  }
  desktop?.publish({ response, frame: { width: frame.width, height: frame.height, source: frame.source }, route: shownRoute() })
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
  state.rejection = null
  setStage('empty')
  status([['Status', 'No frame yet']])
  desktop?.clearOverlay()
}

export async function planAgain(): Promise<void> {
  if (!state.reading || !state.frame) return
  const response = await replan(state.reading, planOptions())
  if (response.status === 'ok') {
    state.plan = response.plan
    if (!response.plan.optimistic) state.optimistic = false
    redraw(false)
    desktop?.publish({ response, frame: { width: state.frame.width, height: state.frame.height, source: state.frame.source }, route: shownRoute() })
  }
}
