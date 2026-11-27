import { $ } from './dom'
import { cellName, drawOverlay, drawSheet } from './draw'
import { shownRoute, state } from './state'
import { CODES, type FrameInfo, type Plan, type Reading, type Rejection, type Route } from './types'

const ms = (v: number): string => (v < 1 ? 'under 1 ms' : v >= 10 ? `${v.toFixed(0)} ms` : `${v.toFixed(1)} ms`)

function fill(id: string, items: Array<[string, string]>): void {
  $<HTMLElement>(id).replaceChildren(
    ...items.map(([k, v]) => {
      const span = document.createElement('span')
      const key = document.createElement('i')
      key.textContent = k
      span.append(key, ` ${v}`)
      return span
    }),
  )
}

export function status(items: Array<[string, string]>): void {
  fill('title-block', items)
}

const versions: { app: string; game: string | null } = { app: __APP_VERSION__, game: null }

/** Shows which Breacher this is and, once a capture has told, which version of the game it looked at. */
export function showVersions(known: { app?: string; game?: string } = {}): void {
  if (known.app) versions.app = known.app
  if (known.game) versions.game = known.game
  fill('versions', [...(versions.game ? ([['Game', versions.game]] as Array<[string, string]>) : []), ['Breacher', versions.app]])
}

function caption(reading: Reading, plan: Plan, route: Route): string {
  const wanted = state.wanted.filter(Boolean).length || reading.sequences.length
  return `${route.steps.length} of ${plan.buffer} picks, ${route.uploaded.length} of ${wanted} daemons`
}

function renderMatrix(reading: Reading, plan: Plan, route: Route, frame: FrameInfo, animate: boolean): void {
  const reduce = window.matchMedia('(prefers-reduced-motion: reduce)').matches
  drawSheet($<SVGSVGElement>('sheet'), reading, route, { animate: animate && !reduce })
  $<HTMLElement>('matrix-size').textContent = `${reading.n} × ${reading.n}`

  // screen view: the route laid over the frame itself
  const hasImage = Boolean(frame.url)
  $<HTMLButtonElement>('tab-screen').disabled = !hasImage
  if (!hasImage && state.view === 'screen') state.view = 'sheet'
  $<HTMLElement>('sheet-wrap').hidden = state.view !== 'sheet'
  $<HTMLElement>('screen-wrap').hidden = state.view !== 'screen'
  $<HTMLButtonElement>('tab-sheet').setAttribute('aria-selected', String(state.view === 'sheet'))
  $<HTMLButtonElement>('tab-screen').setAttribute('aria-selected', String(state.view === 'screen'))
  if (hasImage) {
    const image = $<HTMLImageElement>('screen-image')
    if (image.src !== frame.url) image.src = frame.url ?? ''
    image.alt = `The captured frame, ${frame.width} by ${frame.height}`
    drawOverlay($<SVGSVGElement>('screen-overlay'), reading, route, frame, { caption: caption(reading, plan, route), align: 'start' })
  }
}

function renderBuffer(reading: Reading, plan: Plan, route: Route): void {
  // the buffer: one slot per position, filled in pick order
  $<HTMLElement>('route-count').textContent = `${route.steps.length} of ${plan.buffer} slots used`
  const picks = $<HTMLOListElement>('picks')
  picks.replaceChildren()
  for (let i = 0; i < plan.buffer; i++) {
    const li = document.createElement('li')
    const step = route.steps[i]
    if (step) {
      li.className = 'slot'
      li.innerHTML = `<span class="n">${i + 1}</span><span class="code">${CODES[step.code]}</span><span class="where">${cellName(step.row, step.col)}</span>`
      if (step.uploads.length) {
        li.classList.add('uploads')
        const done = document.createElement('span')
        done.className = 'done'
        done.textContent = `Daemon ${step.uploads.map(d => d + 1).join(' + ')}`
        li.append(done)
      }
    } else {
      li.className = 'slot spare'
      li.setAttribute('aria-label', 'Unused buffer slot')
    }
    picks.append(li)
  }

  $<HTMLOutputElement>('buffer-value').textContent = String(plan.buffer)
  const note = $<HTMLElement>('buffer-note')
  const assumed = Boolean(reading.quality.buffer_assumed)
  note.classList.toggle('warn', assumed && state.buffer === null)
  note.textContent = assumed
    ? state.buffer === null
      ? `The buffer could not be read from this screen. Using ${plan.buffer}: change it if the game shows another number.`
      : 'Set by hand.'
    : state.buffer !== null && state.buffer !== reading.buffer
      ? `The screen shows ${reading.buffer}.`
      : 'Read from the screen.'
  $<HTMLButtonElement>('buffer-down').disabled = plan.buffer <= 1
  $<HTMLButtonElement>('buffer-up').disabled = plan.buffer >= 16
}

function renderDaemons(reading: Reading, route: Route, replan: () => void): void {
  const list = $<HTMLUListElement>('daemons')
  list.replaceChildren()
  const wantedCount = state.wanted.filter(Boolean).length
  $<HTMLElement>('daemon-count').textContent = `${route.uploaded.length} of ${wantedCount} uploaded`
  reading.sequences.forEach((seq, i) => {
    const li = document.createElement('li')
    const at = route.steps.findIndex(s => s.uploads.includes(i))
    const wanted = state.wanted[i] ?? true
    // a code the matrix lacks makes the daemon impossible, and may mean that code was misread
    const foreign = seq.find(c => !reading.matrix.includes(c))
    const fate = !wanted
      ? 'Left out'
      : at >= 0
        ? `Uploads at pick ${at + 1}`
        : foreign !== undefined
          ? `Needs ${CODES[foreign]}, not in the matrix`
          : 'Does not fit'
    li.className = !wanted ? 'off' : at >= 0 ? 'hit' : 'miss'
    const id = `daemon-${i}`
    li.innerHTML = `<label class="check" for="${id}"><input type="checkbox" id="${id}" ${wanted ? 'checked' : ''} /><span class="box" aria-hidden="true"></span><span class="idx">${i + 1}</span><span class="seq">${seq.map(c => `<b>${CODES[c]}</b>`).join('')}</span><span class="status">${fate}</span></label>`
    li.querySelector('input')?.addEventListener('change', e => {
      state.wanted[i] = (e.target as HTMLInputElement).checked
      replan()
    })
    list.append(li)
  })
}

function renderRule(plan: Plan, route: Route, replan: () => void): void {
  const rule = $<HTMLElement>('rule')
  rule.replaceChildren()
  if (plan.optimistic) {
    rule.className = 'rule open'
    const extra = plan.optimistic.uploaded.length - plan.route.uploaded.length
    const p = document.createElement('p')
    p.textContent = state.optimistic
      ? 'This route counts on the game accepting a sequence that overlaps a wrong pick. That is not confirmed, so it may fail.'
      : `A riskier route uploads ${extra} more. It counts on the game accepting a sequence that overlaps a wrong pick, which is not confirmed.`
    const b = document.createElement('button')
    b.textContent = state.optimistic ? 'Show the safe route' : 'Show the risky route'
    b.addEventListener('click', () => {
      state.optimistic = !state.optimistic
      render(true, replan)
    })
    rule.append(p, b)
  } else {
    rule.className = 'rule'
    const p = document.createElement('p')
    p.textContent = !route.uploaded.length
      ? 'Nothing can be uploaded with this buffer.'
      : state.matcher
        ? 'Planned for the rule you set.'
        : 'This route works under all three.'
    rule.append(p)
  }
  if (!plan.proven) {
    const p = document.createElement('p')
    p.textContent = 'The search ran out of time; this is the best route found so far.'
    rule.append(p)
  }
}

/** Draws the current plan; `replan` runs after the player changes which daemons are wanted. */
export function render(animate: boolean, replan: () => void): void {
  const { reading, plan, frame } = state
  const route = shownRoute()
  if (!reading || !plan || !route || !frame) return
  renderMatrix(reading, plan, route, frame, animate)
  renderBuffer(reading, plan, route)
  renderDaemons(reading, route, replan)
  renderRule(plan, route, replan)
  status([
    ['Frame', `${frame.width} × ${frame.height}`],
    ['From', frame.source],
    ['Read and solved in', ms(frame.elapsedMs ?? (reading.timings.total_us + plan.solve_us) / 1000)],
  ])
}

export function renderRejected(response: Rejection, frame: FrameInfo): void {
  $<HTMLElement>('reject-hint').textContent = response.status === 'rejected' ? response.hint : 'That image could not be opened.'
  $<HTMLElement>('reject-detail').textContent = response.status === 'rejected' ? `Stopped at ${response.stage.replace('_', ' ')}: ${response.message}` : response.message
  const img = $<HTMLImageElement>('reject-image')
  img.hidden = !frame.url
  if (frame.url) img.src = frame.url
  status(frame.width ? [['Frame', `${frame.width} × ${frame.height}`], ['From', frame.source]] : [['From', frame.source]])
}
