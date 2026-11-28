import { $ } from './dom'
import { cellName, drawOverlay, drawSheet } from './draw'
import { t, tWhole } from './i18n'
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

/** What was found out about the rule, in words: the rule that is settled, or the one that is ruled out, and `then` what happens about the two that remain. */
export function ruleFinding(known: Matcher, then = 'The next breach goes on with the check.'): string {
  if (settled(known)) return `${t(`Settled: a wrong pick ${RULE_WORDS[known]}.`)} ${t('If routes ever fail to upload what they say, set this back to "Not confirmed yet".')}`
  return `${t(`Ruled out: a wrong pick ${RULE_WORDS[known.replace('not_', '')]}.`)} ${t(then)}`
}

const ms = (v: number): string => (v < 1 ? t('under 1 ms') : v >= 10 ? `${v.toFixed(0)} ms` : `${v.toFixed(1)} ms`)

function fill(id: string, items: Array<[string, string]>): void {
  $<HTMLElement>(id).replaceChildren(
    ...items.map(([k, v]) => {
      const span = document.createElement('span')
      const key = document.createElement('i')
      key.textContent = t(k)
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
  if (state.check) return t('Rule check: play exactly these {n} picks', { n: route.steps.length })
  const wanted = state.wanted.filter(Boolean).length || reading.sequences.length
  return t('{picks} of {buffer} picks, {uploaded} of {daemons} daemons', { picks: route.steps.length, buffer: plan.buffer, uploaded: route.uploaded.length, daemons: wanted })
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
    image.alt = t('The captured frame, {w} by {h}', { w: frame.width, h: frame.height })
    drawOverlay($<SVGSVGElement>('screen-overlay'), reading, route, frame, { caption: caption(reading, plan, route), align: 'start' })
  }
}

function renderBuffer(reading: Reading, plan: Plan, route: Route): void {
  // the buffer: one slot per position, filled in pick order
  $<HTMLElement>('route-count').textContent = t('{used} of {buffer} slots used', { used: route.steps.length, buffer: plan.buffer })
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
        done.textContent = t('Daemon {list}', { list: step.uploads.map(d => d + 1).join(' + ') })
        li.append(done)
      }
    } else {
      li.className = 'slot spare'
      li.setAttribute('aria-label', t('Unused buffer slot'))
    }
    picks.append(li)
  }

  $<HTMLOutputElement>('buffer-value').textContent = String(plan.buffer)
  const note = $<HTMLElement>('buffer-note')
  const assumed = Boolean(reading.quality.buffer_assumed)
  note.classList.toggle('warn', assumed && state.buffer === null)
  note.textContent = assumed
    ? state.buffer === null
      ? t('The buffer could not be read from this screen. Using {n}: change it if the game shows another number.', { n: plan.buffer })
      : t('Set by hand.')
    : state.buffer !== null && state.buffer !== reading.buffer
      ? t('The screen shows {n}.', { n: reading.buffer })
      : t('Read from the screen.')
  $<HTMLButtonElement>('buffer-down').disabled = plan.buffer <= 1
  $<HTMLButtonElement>('buffer-up').disabled = plan.buffer >= 16
}

function renderDaemons(reading: Reading, route: Route, replan: () => void): void {
  const list = $<HTMLUListElement>('daemons')
  list.replaceChildren()
  const wantedCount = state.wanted.filter(Boolean).length
  const check = state.check
  $<HTMLElement>('daemon-count').textContent = check ? t('Rule check') : t('{n} of {m} uploaded', { n: route.uploaded.length, m: wantedCount })
  reading.sequences.forEach((seq, i) => {
    const li = document.createElement('li')
    const at = route.steps.findIndex(s => s.uploads.includes(i))
    const wanted = state.wanted[i] ?? true
    // a code the matrix lacks makes the daemon impossible, and may mean that code was misread
    const foreign = seq.find(c => !reading.matrix.includes(c))
    const tested = check?.outcomes.some(o => o.uploaded.includes(i))
    const fate = check
      ? tested
        ? t('Uploads under some rules')
        : t('Not part of this check')
      : !wanted
        ? t('Left out')
        : at >= 0
          ? t('Uploads at pick {n}', { n: at + 1 })
          : foreign !== undefined
            ? t('Needs {code}, not in the matrix', { code: CODES[foreign] })
            : t('Does not fit')
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

export const daemonNames = (indices: number[]): string => {
  const names = indices.map(i => String(i + 1))
  if (!names.length) return t('Nothing')
  return names.length === 1 ? t('Daemon {a}', { a: names[0] }) : t('Daemons {list} and {last}', { list: names.slice(0, -1).join(', '), last: names[names.length - 1] })
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
      act(t('Something else'), () => hooks.outcome('other')),
      act(t('I did not play it'), () => hooks.outcome('skipped')),
    )
    rule.append(
      say(t('Rule check. Play exactly the {n} picks marked, on a breach you can spare: they are chosen to tell the rules apart, not to upload the most.', { n: route.steps.length })),
      say(t('Then say what the game uploaded:')),
      answers,
    )
    return
  }
  if (plan.optimistic) {
    rule.className = 'rule open'
    const extra = plan.optimistic.uploaded.length - plan.route.uploaded.length
    const p = document.createElement('p')
    p.textContent = state.optimistic
      ? t('This route counts on the game accepting a sequence that overlaps a wrong pick. That is not confirmed, so it may fail.')
      : t('A riskier route uploads {n} more. It counts on the game accepting a sequence that overlaps a wrong pick, which is not confirmed.', { n: extra })
    const b = document.createElement('button')
    b.textContent = state.optimistic ? t('Show the safe route') : t('Show the risky route')
    b.addEventListener('click', () => {
      state.optimistic = !state.optimistic
      render(true, hooks)
    })
    rule.append(p, b)
  } else {
    rule.className = 'rule'
    const p = document.createElement('p')
    p.textContent = !route.uploaded.length
      ? t('Nothing can be uploaded with this buffer.')
      : settled(state.matcher)
        ? t('Planned for the settled rule.')
        : state.matcher
          ? t('This route works under both rules still in play.')
          : t('This route works under all three.')
    rule.append(p)
  }
  if (!plan.proven) {
    const p = document.createElement('p')
    p.textContent = t('The search ran out of time; this is the best route found so far.')
    rule.append(p)
  }
  if (state.ruleNote) rule.append(say(state.ruleNote))
  if (settled(state.matcher)) return
  if (state.checking) {
    // right after an answer the note above already says what happens next
    if (!state.ruleNote) rule.append(say(t('This breach cannot tell the rules still in play apart. Back out before the first pick and open it again for a new matrix, or capture another breach.')))
    rule.append(act(t('Stop checking'), () => hooks.checking(false)))
  } else {
    rule.append(act(t('Check the rule with a breach'), () => hooks.checking(true)))
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
    ['From', tWhole(frame.source)],
    ['Read and solved in', ms(frame.elapsedMs ?? (reading.timings.total_us + plan.solve_us) / 1000)],
  ])
}

export function renderRejected(response: Rejection, frame: FrameInfo): void {
  $<HTMLElement>('reject-hint').textContent = response.status === 'rejected' ? tWhole(response.hint) : t('That image could not be opened.')
  $<HTMLElement>('reject-detail').textContent = response.status === 'rejected' ? t('Stopped at {stage}: {message}', { stage: t(response.stage.replace('_', ' ')), message: response.message }) : response.message
  const img = $<HTMLImageElement>('reject-image')
  img.hidden = !frame.url
  if (frame.url) img.src = frame.url
  status(frame.width ? [['Frame', `${frame.width} × ${frame.height}`], ['From', tWhole(frame.source)]] : [['From', tWhole(frame.source)]])
}
