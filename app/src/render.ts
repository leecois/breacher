import { $ } from './dom'
import { cellName, drawOverlay, drawSheet } from './draw'
import { shownRoute, state } from './state'
import { CODES, settled, type FrameInfo, type Matcher, type Outcome, type Plan, type Reading, type Rejection, type Route } from './types'

/** What the page does when the player acts on something that was drawn. */
export interface Hooks {
  replan(): void
  /** Start or stop spending breaches on checking the wrong-pick rule. */
  checking(on: boolean): void
  /** The player said how a rule check turned out, or that they did not play it. */
  outcome(seen: Outcome | 'other' | 'skipped'): void
}

const RULE_WORDS: Record<string, string> = {
  substring: 'does nothing, and overlapping codes still count',
  retest: 'resets the sequence, which then starts again from that pick',
  strict: 'resets the sequence and is itself lost',
}

/** What a rule check found, in words: the rule it settled, or the one it ruled out. */
export function ruleFinding(known: Matcher): string {
  if (settled(known)) return `Settled: a wrong pick ${RULE_WORDS[known]}. If routes ever fail to upload what they say, set this back to "Not confirmed yet".`
  return `Ruled out: a wrong pick ${RULE_WORDS[known.replace('not_', '')]}. The next breach goes on with the check.`
}

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
  if (state.check) return `Rule check: play exactly these ${route.steps.length} picks`
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
  const check = state.check
  $<HTMLElement>('daemon-count').textContent = check ? 'Rule check' : `${route.uploaded.length} of ${wantedCount} uploaded`
  reading.sequences.forEach((seq, i) => {
    const li = document.createElement('li')
    const at = route.steps.findIndex(s => s.uploads.includes(i))
    const wanted = state.wanted[i] ?? true
    // a code the matrix lacks makes the daemon impossible, and may mean that code was misread
    const foreign = seq.find(c => !reading.matrix.includes(c))
    const tested = check?.outcomes.some(o => o.uploaded.includes(i))
    const fate = check
      ? tested
        ? 'Uploads under some rules'
        : 'Not part of this check'
      : !wanted
        ? 'Left out'
        : at >= 0
          ? `Uploads at pick ${at + 1}`
          : foreign !== undefined
            ? `Needs ${CODES[foreign]}, not in the matrix`
            : 'Does not fit'
    li.className = check ? (tested ? '' : 'off') : !wanted ? 'off' : at >= 0 ? 'hit' : 'miss'
    const id = `daemon-${i}`
    li.innerHTML = `<label class="check" for="${id}"><input type="checkbox" id="${id}" ${wanted ? 'checked' : ''} /><span class="box" aria-hidden="true"></span><span class="idx">${i + 1}</span><span class="seq">${seq.map(c => `<b>${CODES[c]}</b>`).join('')}</span><span class="status">${fate}</span></label>`
    li.querySelector('input')?.addEventListener('change', e => {
      state.wanted[i] = (e.target as HTMLInputElement).checked
      replan()
    })
    list.append(li)
  })
}

function say(text: string): HTMLParagraphElement {
  const p = document.createElement('p')
  p.textContent = text
  return p
}

function act(label: string, run: () => void): HTMLButtonElement {
  const b = document.createElement('button')
  b.textContent = label
  b.addEventListener('click', run)
  return b
}

const daemonNames = (indices: number[]): string => {
  const names = indices.map(i => String(i + 1))
  if (!names.length) return 'Nothing'
  return names.length === 1 ? `Daemon ${names[0]}` : `Daemons ${names.slice(0, -1).join(', ')} and ${names[names.length - 1]}`
}

function renderRule(plan: Plan, route: Route, hooks: Hooks): void {
  const rule = $<HTMLElement>('rule')
  rule.replaceChildren()
  const check = state.check
  if (check) {
    rule.className = 'rule open'
    const answers = document.createElement('div')
    answers.className = 'answers'
    answers.append(
      ...check.outcomes.map(o => act(daemonNames(o.uploaded), () => hooks.outcome(o))),
      act('Something else', () => hooks.outcome('other')),
      act('I did not play it', () => hooks.outcome('skipped')),
    )
    rule.append(
      say(`Rule check. Play exactly the ${route.steps.length} picks marked, on a breach you can spare: they are chosen to tell the rules apart, not to upload the most.`),
      say('Then say what the game uploaded:'),
      answers,
    )
    return
  }
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
      render(true, hooks)
    })
    rule.append(p, b)
  } else {
    rule.className = 'rule'
    const p = document.createElement('p')
    p.textContent = !route.uploaded.length
      ? 'Nothing can be uploaded with this buffer.'
      : settled(state.matcher)
        ? 'Planned for the rule you set.'
        : state.matcher
          ? 'This route works under both rules still in play.'
          : 'This route works under all three.'
    rule.append(p)
  }
  if (!plan.proven) {
    const p = document.createElement('p')
    p.textContent = 'The search ran out of time; this is the best route found so far.'
    rule.append(p)
  }
  if (state.ruleNote) rule.append(say(state.ruleNote))
  if (settled(state.matcher)) return
  if (state.checking) {
    // right after an answer the note above already says what happens next
    if (!state.ruleNote) rule.append(say('This breach cannot tell the rules still in play apart. Back out before the first pick and open it again for a new matrix, or capture another breach.'))
    rule.append(act('Stop checking', () => hooks.checking(false)))
  } else {
    rule.append(act('Check the rule with a breach', () => hooks.checking(true)))
  }
}

/** Draws the current plan, or the rule check that stands in for it. */
export function render(animate: boolean, hooks: Hooks): void {
  const { reading, plan, frame } = state
  const route = shownRoute()
  if (!reading || !plan || !route || !frame) return
  renderMatrix(reading, plan, route, frame, animate)
  renderBuffer(reading, plan, route)
  renderDaemons(reading, route, hooks.replan)
  renderRule(plan, route, hooks)
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
