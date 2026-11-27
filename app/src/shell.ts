import { desktop, type AnswerEvent, type Desktop, type ShellStatus } from './desktop'
import { $ } from './dom'
import { attachImage, show } from './session'
import { remembered, state } from './state'
import type { FrameInfo } from './types'

const MODIFIER = /^(Control|Shift|Alt|Meta|OS)(Left|Right)$/

let current: ShellStatus | null = null
let recording = false

function onAnswer(a: AnswerEvent): void {
  const frame: FrameInfo = { width: a.frame.width, height: a.frame.height, source: a.frame.source, elapsedMs: a.elapsed_ms }
  show(a.response, frame, true)
  // a capture that failed outright has no picture
  if (frame.width) void desktop?.lastFrameUrl().then(url => url && attachImage(frame, url))
}

/** `problem` replaces the shell's own note when a hotkey the player chose could not be bound. */
function showShell(status: ShellStatus, problem?: string): void {
  current = status
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

/** Connects the page to the desktop shell; does nothing in a plain browser. */
export async function startShell(): Promise<void> {
  if (!desktop) return
  const shell = desktop
  showShell(await shell.start(onAnswer))
  wireHotkey(shell)
  $<HTMLButtonElement>('capture').addEventListener('click', () => void shell.capture())
  $<HTMLButtonElement>('allow').addEventListener('click', async () => {
    const status = await shell.allowCapture()
    showShell(status, status.capture_ready ? undefined : 'Allow Breacher under Screen Recording in the system privacy settings, then start it again.')
  })
  $<HTMLInputElement>('overlay-on').addEventListener('change', async e => showShell(await shell.setOverlay((e.target as HTMLInputElement).checked)))
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
  $<HTMLElement>('watch-note').textContent = 'Watching C:\\Users\\you\\Pictures\\Screenshots'
  $<HTMLButtonElement>('watch').textContent = 'Stop watching'
}
