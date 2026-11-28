import { desktop, type AnswerEvent, type Desktop, type ShellStatus, type Targets, type UpdateInfo } from './desktop'
import { $ } from './dom'
import { showVersions, status as say } from './render'
import { attachImage, followed, learnt, show } from './session'
import { remembered, state } from './state'
import type { FrameInfo, Target } from './types'

const MODIFIER = /^(Control|Shift|Alt|Meta|OS)(Left|Right)$/

let current: ShellStatus | null = null
let recording = false
let targets: Targets = { windows: [], displays: [] }

const shorten = (text: string, most = 64): string => (text.length > most ? `${text.slice(0, most - 1)}…` : text)

/** Fills the list of things a capture can be pointed at, keeping the one in use on it even when it is not there right now. */
function showTargets(chosen: Target): void {
  const select = $<HTMLSelectElement>('target')
  const option = (target: Target, label: string): HTMLOptionElement => {
    const o = document.createElement('option')
    o.value = JSON.stringify(target)
    o.textContent = label
    return o
  }
  const group = (label: string, options: HTMLOptionElement[]): HTMLOptGroupElement => {
    const g = document.createElement('optgroup')
    g.label = label
    g.append(...options)
    return g
  }
  const windows = targets.windows.map(w => option({ kind: 'window', value: w.title }, shorten(w.app && !w.title.toLowerCase().includes(w.app.toLowerCase()) ? `${w.title} (${w.app})` : w.title)))
  if (chosen.kind === 'window' && !targets.windows.some(w => w.title === chosen.value)) windows.unshift(option(chosen, `${shorten(chosen.value, 52)} (not open)`))
  const displays = targets.displays.map((d, i) => option({ kind: 'display', value: i }, `Display ${i + 1}, ${d.width} × ${d.height}${d.primary ? ', main' : ''}`))
  if (chosen.kind === 'display' && chosen.value >= targets.displays.length) displays.push(option(chosen, `Display ${chosen.value + 1} (not connected)`))
  select.replaceChildren(option({ kind: 'auto' }, 'The game, found by its name'), ...(windows.length ? [group('A window', windows)] : []), ...(displays.length ? [group('A whole display', displays)] : []))
  select.value = JSON.stringify(chosen)
  const note = $<HTMLElement>('target-note')
  note.hidden = chosen.kind !== 'display'
  note.textContent = 'A whole display is captured once, so the route stays up for a fixed time. Only a window can be followed pick by pick.'
}

function onAnswer(a: AnswerEvent): void {
  const frame: FrameInfo = { width: a.frame.width, height: a.frame.height, source: a.frame.source, elapsedMs: a.elapsed_ms }
  show(a.response, frame, true, a.check ?? null)
  // a capture that failed outright has no picture
  if (frame.width) void desktop?.lastFrameUrl().then(url => url && attachImage(frame, url))
}

/** `problem` replaces the shell's own note when a hotkey the player chose could not be bound. */
function showShell(status: ShellStatus, problem?: string): void {
  current = status
  showVersions({ app: status.version })
  $<HTMLElement>('shell').hidden = false
  const capture = $<HTMLButtonElement>('capture')
  capture.hidden = false
  capture.classList.add('btn')
  capture.classList.toggle('primary', status.capture_ready)
  capture.disabled = !status.capture_ready
  capture.title = status.capture_ready && status.hotkey ? `Same as ${status.hotkey}` : ''
  $<HTMLElement>('hotkey').textContent = status.hotkey
  $<HTMLElement>('way-capture').hidden = !status.capture_ready || !status.hotkey
  const note = $<HTMLElement>('shell-note')
  const text = problem ?? status.capture_note
  note.hidden = !text
  note.textContent = text
  $<HTMLButtonElement>('allow').hidden = status.capture_ready
  $<HTMLElement>('hotkey-set').toggleAttribute('data-recording', recording)
  $<HTMLElement>('hotkey-now').textContent = recording ? 'Press the new keys' : status.hotkey || 'None'
  $<HTMLButtonElement>('hotkey-change').textContent = recording ? 'Cancel' : 'Change'
  $<HTMLInputElement>('overlay-on').checked = status.overlay
  $<HTMLInputElement>('updates-on').checked = status.updates
  showTargets(status.target)
  $<HTMLElement>('auto-set').hidden = status.auto === null
  $<HTMLInputElement>('auto-on').checked = status.auto === true
  $<HTMLElement>('tray-set').hidden = status.tray === null
  $<HTMLInputElement>('tray-on').checked = status.tray === true
  $<HTMLElement>('autostart-set').hidden = status.autostart === null
  $<HTMLInputElement>('autostart-on').checked = status.autostart === true
  $<HTMLButtonElement>('report').hidden = false
  if (status.matcher !== state.matcher) {
    state.matcher = status.matcher
    remembered.matcher = status.matcher
    $<HTMLSelectElement>('matcher').value = status.matcher ?? ''
  }
  const watching = Boolean(status.watch_dir)
  $<HTMLElement>('watch-note').textContent = watching ? `Watching ${status.watch_dir}` : ''
  $<HTMLButtonElement>('watch').textContent = watching ? 'Stop watching' : 'Watch a screenshot folder'
  $<HTMLButtonElement>('watch').dataset.watching = String(watching)
  $<HTMLElement>('way-watch').hidden = !watching
  // the log's location, for bug reports
  $<HTMLElement>('shell').title = `Log: ${status.log_path}`
}

/** The pressed combination in the form the shell parses, or null while only modifiers are down. */
function accelerator(e: KeyboardEvent): string | null {
  if (!e.code || MODIFIER.test(e.code)) return null
  const held = [e.ctrlKey && 'Control', e.altKey && 'Alt', e.shiftKey && 'Shift', e.metaKey && 'Super'].filter(Boolean)
  return [...held, e.code].join('+')
}

/** Ends a hotkey change: binds `keys`, or takes the old hotkey back with null. */
async function finish(shell: Desktop, keys: string | null): Promise<void> {
  recording = false
  try {
    showShell(await shell.setHotkey(keys))
  } catch (err) {
    showShell(await shell.status(), String(err))
  }
}

function wireHotkey(shell: Desktop): void {
  $<HTMLButtonElement>('hotkey-change').addEventListener('click', async () => {
    if (recording) return void finish(shell, null)
    recording = true
    await shell.pauseHotkey()
    if (current) showShell(current)
  })
  window.addEventListener(
    'keydown',
    e => {
      if (!recording) return
      e.preventDefault()
      e.stopPropagation()
      const bare = !(e.ctrlKey || e.altKey || e.shiftKey || e.metaKey)
      if (e.code === 'Escape' && bare) return void finish(shell, null)
      const keys = accelerator(e)
      if (keys) void finish(shell, keys)
    },
    true,
  )
  // a hotkey left paused would stay dead once the player is back in the game
  window.addEventListener('blur', () => {
    if (recording) void finish(shell, null)
  })
}

/** Shows a newer release and what the player can do about it; `failure` is why installing it did not work. */
function showUpdate(shell: Desktop, update: UpdateInfo, failure?: string): void {
  const text = $<HTMLElement>('update-text')
  const go = $<HTMLButtonElement>('update-go')
  const install = update.installable && !failure
  $<HTMLElement>('update').hidden = false
  go.hidden = false
  go.disabled = false
  go.textContent = install ? 'Update and restart' : 'Open the download page'
  text.textContent = failure
    ? `Breacher ${update.version} did not install: ${failure}`
    : update.installable
      ? `Breacher ${update.version} is out.`
      : `Breacher ${update.version} is out. This copy runs without installing, so it cannot replace itself.`
  go.onclick = async () => {
    if (!install) return void shell.openDownloads()
    go.disabled = true
    text.textContent = `Downloading Breacher ${update.version}`
    try {
      await shell.installUpdate(percent => {
        text.textContent = percent < 100 ? `Downloading Breacher ${update.version}: ${percent}%` : `Installing Breacher ${update.version}`
      })
    } catch (err) {
      showUpdate(shell, update, String(err))
    }
  }
}

/** Connects the page to the desktop shell; does nothing in a plain browser. */
export async function startShell(): Promise<void> {
  if (!desktop) return
  const shell = desktop
  showShell(
    await shell.start(
      onAnswer,
      game => showVersions({ game }),
      p => followed(p.route, p.replanned),
      rule => void learnt(rule),
    ),
  )
  wireHotkey(shell)
  const lookForUpdate = (): void => {
    void shell
      .checkUpdate()
      .then(update => update && showUpdate(shell, update))
      .catch(() => undefined)
  }
  lookForUpdate()
  $<HTMLInputElement>('updates-on').addEventListener('change', async e => {
    const status = await shell.setUpdates((e.target as HTMLInputElement).checked)
    showShell(status)
    $<HTMLElement>('update').hidden = true
    if (status.updates) lookForUpdate()
  })
  $<HTMLButtonElement>('capture').addEventListener('click', () => void shell.capture())
  $<HTMLButtonElement>('allow').addEventListener('click', async () => {
    const status = await shell.allowCapture()
    showShell(status, status.capture_ready ? undefined : 'Allow Breacher under Screen Recording in the system privacy settings, then start it again.')
  })
  $<HTMLInputElement>('overlay-on').addEventListener('change', async e => showShell(await shell.setOverlay((e.target as HTMLInputElement).checked)))
  const target = $<HTMLSelectElement>('target')
  const listTargets = async (): Promise<void> => {
    targets = await shell.listTargets().catch(() => targets)
    // a list that is open is left alone: rebuilding it would close it under the pointer
    if (current && document.activeElement !== target) showTargets(current.target)
  }
  void listTargets()
  // windows come and go, so the list is made anew each time the player turns to it
  target.addEventListener('pointerenter', () => void listTargets())
  window.addEventListener('focus', () => void listTargets())
  target.addEventListener('change', async () => {
    showShell(await shell.setTarget(JSON.parse(target.value) as Target))
    target.blur()
  })
  $<HTMLInputElement>('auto-on').addEventListener('change', async e => showShell(await shell.setAuto((e.target as HTMLInputElement).checked)))
  $<HTMLInputElement>('tray-on').addEventListener('change', async e => showShell(await shell.setTray((e.target as HTMLInputElement).checked)))
  $<HTMLInputElement>('autostart-on').addEventListener('change', async e => {
    try {
      showShell(await shell.setAutostart((e.target as HTMLInputElement).checked))
    } catch (err) {
      showShell(await shell.status(), `Starting at sign-in could not be changed: ${String(err)}`)
    }
  })
  $<HTMLButtonElement>('report').addEventListener('click', async () => {
    const now = new Date()
    const two = (n: number): string => String(n).padStart(2, '0')
    const stamp = `${now.getFullYear()}-${two(now.getMonth() + 1)}-${two(now.getDate())} ${two(now.getHours())}-${two(now.getMinutes())}-${two(now.getSeconds())}`
    try {
      say([['Report saved in', await shell.saveReport(stamp)]])
    } catch (err) {
      say([['Report not saved', String(err)]])
    }
  })
  $<HTMLButtonElement>('watch').addEventListener('click', async e => {
    const watching = (e.currentTarget as HTMLButtonElement).dataset.watching === 'true'
    const dir = watching ? null : await shell.pickFolder()
    if (watching || dir) {
      try {
        showShell(await shell.watchFolder(dir))
      } catch (err) {
        $<HTMLElement>('watch-note').textContent = String(err)
      }
    }
  })
}

/** Shows the desktop-only controls in a browser, to check the layout. */
export function previewShell(): void {
  $<HTMLElement>('shell').hidden = false
  $<HTMLButtonElement>('capture').hidden = false
  $<HTMLElement>('way-capture').hidden = false
  $<HTMLElement>('hotkey-now').textContent = 'Ctrl Shift B'
  $<HTMLInputElement>('overlay-on').checked = true
  $<HTMLInputElement>('updates-on').checked = true
  targets = {
    windows: [
      { title: 'Cyberpunk 2077', app: 'Cyberpunk2077.exe' },
      { title: 'Friends and chat', app: 'Launcher' },
    ],
    displays: [
      { width: 2560, height: 1440, primary: true },
      { width: 1920, height: 1080, primary: false },
    ],
  }
  showTargets(new URLSearchParams(location.search).get('target') === 'display' ? { kind: 'display', value: 1 } : { kind: 'auto' })
  $<HTMLElement>('auto-set').hidden = false
  $<HTMLElement>('tray-set').hidden = false
  $<HTMLInputElement>('tray-on').checked = true
  $<HTMLElement>('autostart-set').hidden = false
  $<HTMLButtonElement>('report').hidden = false
  $<HTMLElement>('watch-note').textContent = 'Watching C:\\Users\\you\\Pictures\\Screenshots'
  $<HTMLButtonElement>('watch').textContent = 'Stop watching'
}
