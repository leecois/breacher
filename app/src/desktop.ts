// Bridge to the desktop shell; in a plain browser `desktop` is null and the page works on dropped or pasted screenshots.
import type { Matcher, Priority, Response, Route } from './types'

export interface AnswerEvent {
  response: Response
  frame: { width: number; height: number; source: string }
  elapsed_ms: number
}

export interface OverlayPayload {
  response: Response
  frame: { width: number; height: number; source: string }
  /** The route currently shown in the main window (may be the risky alternative). */
  route: Route | null
}

export interface ShellStatus {
  hotkey: string
  capture_ready: boolean
  /** Why capture is unavailable, in words for the player. */
  capture_note: string
  watch_dir: string | null
  overlay: boolean
  matcher: Matcher | null
  /** Where the shell writes its log, for bug reports. */
  log_path: string
}

export interface Desktop {
  start(onAnswer: (a: AnswerEvent) => void): Promise<ShellStatus>
  capture(): Promise<void>
  publish(payload: OverlayPayload): void
  setPriority(p: Priority): void
  setMatcher(m: Matcher | null): void
  lastFrameUrl(): Promise<string | null>
  /** Ask the system for screen-capture permission (shows the system prompt). */
  allowCapture(): Promise<ShellStatus>
  setOverlay(enabled: boolean): Promise<ShellStatus>
  /** Let the player choose a folder; resolves to null when they cancel. */
  pickFolder(): Promise<string | null>
  /** Watch a folder for new screenshots, or stop watching with null. */
  watchFolder(dir: string | null): Promise<ShellStatus>
}

const inShell = '__TAURI_INTERNALS__' in window

function make(): Desktop {
  const core = import('@tauri-apps/api/core')
  const event = import('@tauri-apps/api/event')
  return {
    async start(onAnswer) {
      const { listen } = await event
      await listen<AnswerEvent>('answer', e => onAnswer(e.payload))
      const { invoke } = await core
      return invoke<ShellStatus>('status')
    },
    async capture() {
      const { invoke } = await core
      await invoke('capture')
    },
    publish(payload) {
      void event.then(({ emitTo }) => emitTo('overlay', 'route', payload)).catch(() => undefined)
    },
    setPriority(p) {
      void core.then(({ invoke }) => invoke('set_priority', { priority: p })).catch(() => undefined)
    },
    setMatcher(m) {
      void core.then(({ invoke }) => invoke('set_matcher', { matcher: m })).catch(() => undefined)
    },
    async lastFrameUrl() {
      const { invoke } = await core
      return invoke<string | null>('last_frame_url')
    },
    async allowCapture() {
      const { invoke } = await core
      return invoke<ShellStatus>('allow_capture')
    },
    async setOverlay(enabled) {
      const { invoke } = await core
      return invoke<ShellStatus>('set_overlay', { enabled })
    },
    async pickFolder() {
      const { open } = await import('@tauri-apps/plugin-dialog')
      const picked = await open({ directory: true, multiple: false, title: 'Folder where screenshots are saved' })
      return typeof picked === 'string' ? picked : null
    },
    async watchFolder(dir) {
      const { invoke } = await core
      return invoke<ShellStatus>('watch_folder', { dir })
    },
  }
}

export const desktop: Desktop | null = inShell ? make() : null
