// Bridge to the desktop shell; in a plain browser `desktop` is null and the page works on dropped or pasted screenshots.
import type { Matcher, Priority, Response, Route, RuleCheck, Target } from './types'

export interface AnswerEvent {
  response: Response
  frame: { width: number; height: number; source: string }
  elapsed_ms: number
  /** The rule check to play on this screen in place of the route, when one was asked for. */
  check?: RuleCheck | null
}

export interface OverlayPayload {
  response: Response
  frame: { width: number; height: number; source: string }
  /** The route currently shown in the main window (may be the risky alternative). */
  route: Route | null
  /** The route is a rule check, to be played exactly as drawn. */
  check?: boolean
}

/** News of a pick the shell saw in the game. */
export interface ProgressEvent {
  /** The route as it now stands, with the picks made counted off. */
  route: Route
  /** The pick left the route that was drawn, and this one replaces it. */
  replanned: boolean
  check: boolean
}

export interface ShellStatus {
  /** The version of the desktop program. */
  version: string
  /** The bound hotkey as the player reads it; empty when none could be bound. */
  hotkey: string
  capture_ready: boolean
  /** What the player should know about capture or the hotkey; empty when all is well. */
  capture_note: string
  watch_dir: string | null
  overlay: boolean
  /** Ask the release page for a newer version at start. */
  updates: boolean
  /** What a capture is pointed at. */
  target: Target
  /** Closing the window keeps the program in the tray; null where there is no tray. */
  tray: boolean | null
  /** The program starts when the player signs in; null when the system will not say. */
  autostart: boolean | null
  matcher: Matcher | null
  /** Where the shell writes its log, for bug reports. */
  log_path: string
}

/** What a capture can be pointed at right now. */
export interface Targets {
  windows: Array<{ title: string; app: string }>
  /** Displays in the order a capture counts them: the main one first. */
  displays: Array<{ width: number; height: number; primary: boolean }>
}

export interface UpdateInfo {
  version: string
  /** False for a copy that was never installed: it cannot replace itself. */
  installable: boolean
}

export interface Desktop {
  /** `onGame` hears the version of the game each time a capture finds its window, `onProgress` each pick seen in it. */
  start(onAnswer: (a: AnswerEvent) => void, onGame: (version: string) => void, onProgress: (p: ProgressEvent) => void): Promise<ShellStatus>
  /** Say which route the window shows and under which choices, so that picks are followed against that one. */
  follow(route: Route, wanted: boolean[] | null, buffer: number | null, check: boolean): void
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
  /** Spend the breaches captured from now on checking the wrong-pick rule, or stop doing so. */
  setChecking(on: boolean): void
  lastFrameUrl(): Promise<string | null>
  /** Ask the system for screen-capture permission (shows the system prompt). */
  allowCapture(): Promise<ShellStatus>
  setOverlay(enabled: boolean): Promise<ShellStatus>
  /** The windows and displays that are there to capture at this moment. */
  listTargets(): Promise<Targets>
  setTarget(target: Target): Promise<ShellStatus>
  setUpdates(enabled: boolean): Promise<ShellStatus>
  setTray(enabled: boolean): Promise<ShellStatus>
  setAutostart(enabled: boolean): Promise<ShellStatus>
  /** Save the last frame, its reading, the log and the versions to a folder; resolves to that folder. */
  saveReport(stamp: string): Promise<string>
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
    async start(onAnswer, onGame, onProgress) {
      const { listen } = await event
      await listen<AnswerEvent>('answer', e => onAnswer(e.payload))
      await listen<string>('game-version', e => onGame(e.payload))
      await listen<ProgressEvent>('progress', e => onProgress(e.payload))
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
    follow(route, wanted, buffer, check) {
      void core.then(({ invoke }) => invoke('shown_route', { route, wanted, buffer, check })).catch(() => undefined)
    },
    clearOverlay() {
      void core.then(({ invoke }) => invoke('dismiss_overlay')).catch(() => undefined)
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
    setChecking(on) {
      void core.then(({ invoke }) => invoke('set_checking', { on })).catch(() => undefined)
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
    async listTargets() {
      const { invoke } = await core
      return invoke<Targets>('list_targets')
    },
    async setTarget(target) {
      const { invoke } = await core
      return invoke<ShellStatus>('set_target', { target })
    },
    async setUpdates(enabled) {
      const { invoke } = await core
      return invoke<ShellStatus>('set_updates', { enabled })
    },
    async setTray(enabled) {
      const { invoke } = await core
      return invoke<ShellStatus>('set_tray', { enabled })
    },
    async setAutostart(enabled) {
      const { invoke } = await core
      return invoke<ShellStatus>('set_autostart', { enabled })
    },
    async saveReport(stamp) {
      const { invoke } = await core
      return invoke<string>('save_report', { stamp })
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
