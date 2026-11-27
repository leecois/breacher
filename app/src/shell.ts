import { desktop, type AnswerEvent, type ShellStatus } from './desktop'
import { $ } from './dom'
import { show } from './session'
import { remembered, state } from './state'

function onAnswer(a: AnswerEvent): void {
  show(a.response, { width: a.frame.width, height: a.frame.height, source: a.frame.source, elapsedMs: a.elapsed_ms }, true)
}

function showShell(status: ShellStatus): void {
  $<HTMLElement>('shell').hidden = false
  const capture = $<HTMLButtonElement>('capture')
  capture.hidden = false
  capture.classList.add('btn')
  capture.classList.toggle('primary', status.capture_ready)
  capture.disabled = !status.capture_ready
  capture.title = status.capture_ready ? `Same as ${status.hotkey}` : ''
  $<HTMLElement>('hotkey').textContent = status.hotkey
  $<HTMLElement>('way-capture').hidden = !status.capture_ready
  const note = $<HTMLElement>('shell-note')
  note.hidden = !status.capture_note
  note.textContent = status.capture_note
  $<HTMLButtonElement>('allow').hidden = status.capture_ready
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

/** Connects the page to the desktop shell; does nothing in a plain browser. */
export async function startShell(): Promise<void> {
  if (!desktop) return
  const shell = desktop
  showShell(await shell.start(onAnswer))
  $<HTMLButtonElement>('capture').addEventListener('click', () => void shell.capture())
  $<HTMLButtonElement>('allow').addEventListener('click', async () => {
    const status = await shell.allowCapture()
    showShell(status)
    if (!status.capture_ready) $<HTMLElement>('shell-note').textContent = 'Allow Breacher under Screen Recording in the system privacy settings, then start it again.'
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
  $<HTMLInputElement>('overlay-on').checked = true
  $<HTMLElement>('watch-note').textContent = 'Watching C:\\Users\\you\\Pictures\\Screenshots'
  $<HTMLButtonElement>('watch').textContent = 'Stop watching'
}
