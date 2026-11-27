import { pixelsOf, readRgba, replan } from './core'
import { desktop } from './desktop'
import { $ } from './dom'
import { render, renderRejected } from './render'
import { planOptions, remembered, shownRoute, state } from './state'
import type { FrameInfo, Response } from './types'

const stage = $<HTMLElement>('stage')

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
    stage.dataset.state = 'result'
    redraw(fresh)
  } else {
    state.reading = null
    state.plan = null
    stage.dataset.state = 'rejected'
    renderRejected(response, frame)
  }
  desktop?.publish({ response, frame: { width: frame.width, height: frame.height, source: frame.source }, route: shownRoute() })
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
