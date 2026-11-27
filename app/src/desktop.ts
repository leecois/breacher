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
  /** The bound hotkey as the player reads it; empty when none could be bound. */
  hotkey: string
  capture_ready: boolean
  /** What the player should know about capture or the hotkey; empty when all is well. */
  capture_note: string
  watch_dir: string | null
  overlay: boolean
  /** Ask the release page for a newer version at start. */
  updates: boolean
  matcher: Matcher | null
  /** Where the shell writes its log, for bug reports. */
  log_path: string
}

export interface UpdateInfo {
  version: string
  /** False for a copy that was never installed: it cannot replace itself. */
  installable: boolean
}

export interface Desktop {
  start(onAnswer: (a: AnswerEvent) => void): Promise<ShellStatus>
  status(): Promise<ShellStatus>
  capture(): Promise<void>
  publish(payload: OverlayPayload): void
  /** Wipe the route drawn over the game. */
  clearOverlay(): void
  /** Let go of the hotkey so the page can see the keys of a new one. */
  pauseHotkey(): Promise<void>
  /** Bind a new hotkey, or take the current one back with null; rejects with words for the player. */
  setHotkey(accelerator: string | null): Promise<ShellStatus>
  /** The newer release, or null when there is none or the release page cannot be reached. */
  checkUpdate(): Promise<UpdateInfo | null>
  /** Download and install the update found by the last check; the program restarts when it succeeds. */
  installUpdate(onProgress: (percent: number) => void): Promise<void>
  openDownloads(): Promise<void>
  setPriority(p: Priority): void
  setMatcher(m: Matcher | null): void
  lastFrameUrl(): Promise<string | null>
  /** Ask the system for screen-capture permission (shows the system prompt). */
  allowCapture(): Promise<ShellStatus>
  setOverlay(enabled: boolean): Promise<ShellStatus>
  setUpdates(enabled: boolean): Promise<ShellStatus>
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
    async status() {
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
    clearOverlay() {
      void event.then(({ emitTo }) => emitTo('overlay', 'clear')).catch(() => undefined)
    },
    async pauseHotkey() {
      const { invoke } = await core
      await invoke('pause_hotkey')
    },
    async setHotkey(accelerator) {
      const { invoke } = await core
      return invoke<ShellStatus>('set_hotkey', { accelerator })
    },
    async checkUpdate() {
      const { invoke } = await core
      return invoke<UpdateInfo | null>('check_update')
    },
    async installUpdate(onProgress) {
      const { listen } = await event
      const stop = await listen<number>('update-progress', e => onProgress(e.payload))
      const { invoke } = await core
      try {
        await invoke('install_update')
      } finally {
        stop()
      }
    },
    async openDownloads() {
      const { invoke } = await core
      await invoke('open_downloads')
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
    async setUpdates(enabled) {
      const { invoke } = await core
      return invoke<ShellStatus>('set_updates', { enabled })
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
