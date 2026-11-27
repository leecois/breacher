import { desktop } from './desktop'
import { $ } from './dom'
import { home, planAgain, readBlob, redraw } from './session'
import { remembered, state } from './state'
import { asMatcher, settled, type Priority } from './types'

export function wireInput(): void {
  const file = $<HTMLInputElement>('file')
  $<HTMLButtonElement>('home').addEventListener('click', home)
  $<HTMLButtonElement>('open').addEventListener('click', () => file.click())
  file.addEventListener('change', () => {
    const f = file.files?.[0]
    if (f) void readBlob(f, f.name)
    file.value = ''
  })

  const veil = $<HTMLElement>('drop-veil')
  let dragDepth = 0
  window.addEventListener('dragenter', e => {
    if (e.dataTransfer?.types.includes('Files')) {
      dragDepth++
      veil.classList.add('on')
    }
  })
  window.addEventListener('dragleave', () => {
    dragDepth = Math.max(0, dragDepth - 1)
    if (!dragDepth) veil.classList.remove('on')
  })
  window.addEventListener('dragover', e => e.preventDefault())
  window.addEventListener('drop', e => {
    e.preventDefault()
    dragDepth = 0
    veil.classList.remove('on')
    const f = [...(e.dataTransfer?.files ?? [])].find(f => f.type.startsWith('image/'))
    if (f) void readBlob(f, f.name)
  })
  window.addEventListener('paste', e => {
    const item = [...(e.clipboardData?.items ?? [])].find(i => i.type.startsWith('image/'))
    const f = item?.getAsFile()
    if (f) {
      e.preventDefault()
      void readBlob(f, 'Clipboard')
    }
  })
}

export function wireSettings(): void {
  $<HTMLButtonElement>('buffer-down').addEventListener('click', () => {
    if (state.plan) {
      state.buffer = Math.max(1, state.plan.buffer - 1)
      void planAgain()
    }
  })
  $<HTMLButtonElement>('buffer-up').addEventListener('click', () => {
    if (state.plan) {
      state.buffer = Math.min(16, state.plan.buffer + 1)
      void planAgain()
    }
  })
  document.querySelectorAll<HTMLInputElement>('input[name="priority"]').forEach(r =>
    r.addEventListener('change', () => {
      if (r.checked) {
        state.priority = r.value as Priority
        desktop?.setPriority(state.priority)
        void planAgain()
      }
    }),
  )
  const matcher = $<HTMLSelectElement>('matcher')
  matcher.value = state.matcher ?? ''
  matcher.addEventListener('change', () => {
    state.matcher = asMatcher(matcher.value)
    remembered.matcher = state.matcher
    desktop?.setMatcher(state.matcher)
    // a rule set by hand overrides a check that is under way
    state.check = null
    state.ruleNote = ''
    if (settled(state.matcher) && state.checking) {
      state.checking = false
      desktop?.setChecking(false)
    }
    state.optimistic = false
    void planAgain()
  })
  $<HTMLButtonElement>('tab-sheet').addEventListener('click', () => {
    state.view = 'sheet'
    redraw(false)
  })
  $<HTMLButtonElement>('tab-screen').addEventListener('click', () => {
    state.view = 'screen'
    redraw(false)
  })
}
