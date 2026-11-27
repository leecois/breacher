// Screen sharing in a plain browser: the page watches the shared window or screen, reads each breach as it opens and follows the picks.
import { follow } from './core'
import { desktop } from './desktop'
import { $ } from './dom'
import { canFloat, closeFloat, isFloating, openFloat, paintFloat } from './float'
import { daemonNames, status } from './render'
import { attachImage, followed, readPixels, showRead } from './session'
import { shownRoute, state } from './state'
import type { Fate, FrameInfo, Response } from './types'

/** Milliseconds between two looks at the shared picture. */
const TICK = 200
/** Looks in a row without the matrix before a breach being played is taken to have closed. */
const GONE = 8
const [SMALL_W, SMALL_H] = [48, 27]
/** Mean difference per pixel, out of 255, under which two glances show a picture at rest; an open breach stays under 2. */
const AT_REST = 3
/** Mean difference per pixel over which the picture is no longer the one last tried. */
const CHANGED = 4
/** Looks after which the picture is tried whatever it looks like, in case its coming to rest was missed. */
const ANYWAY = 15

interface Sharing {
  stream: MediaStream
  /** A second handle on the picture, for the browsers that hand frames over one by one. */
  tap: MediaStreamTrack | null
  newest: VideoFrame | null
  ticker: Worker
  /** What the player chose to share, in a word. */
  surface: string
}

type FrameSource = new (init: { track: MediaStreamTrack }) => { readable: ReadableStream<VideoFrame> }

let sharing: Sharing | null = null
let busy = false
/** The last glance, and the one the reader was last tried on. */
let last: Uint8Array | null = null
let tried: Uint8Array | null = null
let sinceTry = 0
/** What an unasked read made of the screen, kept until a second read agrees: a screen still opening must not be taken at its word. */
let unconfirmed: string | null = null
/** The breach being followed, and the one the player put away, which is not brought back while it is still on screen. */
let liveIs: string | null = null
let putAway: string | null = null

/** What a read made of the screen, in a form two reads can be compared by; null when it is no breach. */
const madeOf = (response: Response): string | null => (response.status === 'ok' ? JSON.stringify([response.reading.n, response.reading.matrix, response.reading.sequences, response.reading.buffer]) : null)
/** The frame of the breach being followed; it is followed for as long as the page shows it. */
let live: FrameInfo | null = null
let played: number[] = []
let fates: Fate[] = []
let gone = 0

const full = document.createElement('canvas')
const small = document.createElement('canvas')
small.width = SMALL_W
small.height = SMALL_H

/** Whether this page can be shown the game: a browser, with screen sharing. */
export const canShare = !desktop && typeof navigator.mediaDevices?.getDisplayMedia === 'function'

function say(text: string): void {
  $<HTMLElement>('live-state').textContent = text
  // the floating window says the same while it has no route to show
  paintFloat(text)
}

const watchingWords = (): string => ($<HTMLInputElement>('live-auto').checked ? 'Watching for a breach.' : 'Open a breach in the game, then press Read now.')

function picture(): { from: CanvasImageSource; width: number; height: number } | null {
  if (!sharing) return null
  if (sharing.newest) return { from: sharing.newest, width: sharing.newest.displayWidth, height: sharing.newest.displayHeight }
  const video = $<HTMLVideoElement>('live-view')
  return video.videoWidth ? { from: video, width: video.videoWidth, height: video.videoHeight } : null
}

/** The shared picture as it is now, in full. */
function pixels(): ImageData | null {
  const now = picture()
  const ctx = full.getContext('2d', { willReadFrequently: true })
  if (!now || !ctx) return null
  if (full.width !== now.width || full.height !== now.height) [full.width, full.height] = [now.width, now.height]
  ctx.drawImage(now.from, 0, 0)
  return ctx.getImageData(0, 0, now.width, now.height)
}

/** The shared picture shrunk to a few hundred brightness values: enough to tell whether it moved. */
function glance(): Uint8Array | null {
  const now = picture()
  const ctx = small.getContext('2d', { willReadFrequently: true })
  if (!now || !ctx) return null
  // every pixel has to weigh in, or the faint moving background of a breach reads as motion
  ctx.imageSmoothingQuality = 'high'
  ctx.drawImage(now.from, 0, 0, SMALL_W, SMALL_H)
  const rgba = ctx.getImageData(0, 0, SMALL_W, SMALL_H).data
  const out = new Uint8Array(SMALL_W * SMALL_H)
  for (let i = 0; i < out.length; i++) out[i] = Math.max(rgba[4 * i], rgba[4 * i + 1], rgba[4 * i + 2])
  return out
}

function apart(a: Uint8Array, b: Uint8Array): number {
  let sum = 0
  for (let i = 0; i < a.length; i++) sum += Math.abs(a[i] - b[i])
  return sum / a.length
}

/** Gives the frame its picture once the browser has encoded it: the canvas is copied at the call, before the next look draws over it. */
function keepPicture(frame: FrameInfo): void {
  full.toBlob(blob => blob && attachImage(frame, URL.createObjectURL(blob)), 'image/jpeg', 0.9)
}

/** Goes back to watching. With `again`, whatever is on screen next is worth one try, even if it looks like what was read before. */
function watch(words: string, again = true): void {
  live = null
  played = []
  fates = []
  gone = 0
  tried = again ? null : last
  unconfirmed = null
  say(words)
}

/** Reads the shared picture as it is now. A read the player asked for is shown whatever came of it; an unasked one only once two reads in a row agree, and not at all when the screen is no breach. */
async function read(asked: boolean): Promise<void> {
  const image = pixels()
  if (!image || !sharing) {
    if (asked) say('No picture has arrived yet. Try again in a moment.')
    return
  }
  const source = `Shared ${sharing.surface}`
  const { response, elapsedMs } = await readPixels(image.data, image.width, image.height)
  const made = madeOf(response)
  if (!asked) {
    const agreed = made !== null && made === unconfirmed
    if (made !== putAway) putAway = null
    unconfirmed = made === putAway ? null : made
    if (!agreed || made === putAway) return
  }
  unconfirmed = null
  putAway = null
  const frame: FrameInfo = { width: image.width, height: image.height, source, elapsedMs }
  keepPicture(frame)
  await showRead(response, frame)
  if (state.reading && state.frame === frame) {
    live = frame
    liveIs = made
    played = []
    fates = []
    gone = 0
    say('Breach read. The picks you make are counted off.')
  } else {
    watch(watchingWords())
  }
}

function outcomeWords(): string {
  const of = (fate: Fate): number[] => fates.flatMap((f, i) => (f === fate ? [i] : []))
  const [installed, failed] = [of('installed'), of('failed')]
  return `Breach over: ${daemonNames(installed).toLowerCase()} installed${failed.length ? `, ${daemonNames(failed).toLowerCase()} failed` : ''}.`
}

/** Looks at the breach being followed: counts off new picks, plans again after a pick off the route, and notices the end. */
async function look(): Promise<void> {
  const [reading, route] = [state.reading, shownRoute()]
  // the player has put the breach away or opened another picture: it is not brought back while it is still on screen
  if (!live || !reading || !route || state.frame !== live) {
    putAway = liveIs
    return watch(watchingWords(), false)
  }
  const image = pixels()
  if (!image) return
  // a picture of another size is not the screen that was read
  const same = image.width === live.width && image.height === live.height
  const seen = same
    ? await follow(image.data, image.width, image.height, {
        reading,
        route,
        check: Boolean(state.check),
        wanted: state.wanted.length ? state.wanted : null,
        buffer: state.buffer,
        played,
        fates,
        priority: state.priority,
        matcher: state.matcher,
      })
    : null
  if (state.frame !== live) return
  if (!seen || seen.sight === 'other') {
    gone++
    // before the first pick the breach can be closed and opened again for a new matrix, so one look without it is enough
    if (gone >= GONE || !played.length) watch(played.length ? 'The breach screen closed.' : watchingWords())
    return
  }
  gone = 0
  played = seen.played
  fates = seen.fates
  if (seen.step) followed(seen.route, seen.step === 'replanned')
  if (seen.sight === 'over') return watch(outcomeWords())
  if (seen.step) {
    const [done, picks] = [seen.route.done ?? 0, seen.route.steps.length]
    say(seen.step === 'replanned' ? `Off the route after ${played.length} ${played.length === 1 ? 'pick' : 'picks'}. The rest is planned again.` : done >= picks ? `All ${picks} picks made.` : `Pick ${done} seen. Pick ${done + 1} of ${picks} is next.`)
  }
}

async function tick(): Promise<void> {
  if (busy || !sharing) return
  busy = true
  try {
    if (live) return await look()
    if (!$<HTMLInputElement>('live-auto').checked) return
    const now = glance()
    if (!now) return
    const atRest = last !== null && apart(now, last) < AT_REST
    const fresh = tried === null || apart(now, tried) > CHANGED
    last = now
    sinceTry++
    if (unconfirmed !== null || (atRest && fresh) || sinceTry >= ANYWAY) {
      tried = now
      sinceTry = 0
      await read(false)
    }
  } catch (e) {
    say(`The shared picture could not be read: ${e instanceof Error ? e.message : String(e)}`)
  } finally {
    busy = false
  }
}

/** Keeps the newest frame at hand where the browser hands frames over one by one, which does not depend on the page being in view. */
function tap(into: Sharing): void {
  const Source = (window as unknown as { MediaStreamTrackProcessor?: FrameSource }).MediaStreamTrackProcessor
  const track = into.stream.getVideoTracks()[0]
  if (!Source || !track) return
  try {
    into.tap = track.clone()
    const reader = new Source({ track: into.tap }).readable.getReader()
    void (async () => {
      for (;;) {
        const { value, done } = await reader.read()
        if (done || !value) return
        if (sharing !== into) return value.close()
        into.newest?.close()
        into.newest = value
      }
    })().catch(() => undefined)
  } catch {
    // the picture is then taken from the preview
    into.tap?.stop()
    into.tap = null
  }
}

function showSharing(): void {
  const on = sharing !== null
  $<HTMLElement>('live').hidden = !on
  $<HTMLButtonElement>('share').textContent = on ? 'Read now' : 'Share the game screen'
  // a shared screen shows the floating window too, so it must not sit on what is to be read
  const aside = isFloating() && sharing?.surface === 'screen' ? ' Keep the floating window off the code matrix.' : ''
  $<HTMLElement>('live-what').textContent = on ? `Sharing a ${sharing?.surface}. Nothing leaves this page: the picture is read on your computer.${aside}` : ''
  $<HTMLButtonElement>('live-float').hidden = !on || !canFloat
  $<HTMLButtonElement>('live-float').textContent = isFloating() ? 'Close the floating route' : 'Float the route over the game'
}

function stop(): void {
  if (!sharing) return
  const was = sharing
  sharing = null
  was.ticker.terminate()
  was.tap?.stop()
  was.newest?.close()
  was.stream.getTracks().forEach(track => track.stop())
  $<HTMLVideoElement>('live-view').srcObject = null
  live = null
  last = null
  tried = null
  unconfirmed = null
  putAway = null
  closeFloat()
  showSharing()
}

async function share(): Promise<void> {
  // the player chooses a window or a screen in the browser's own dialog; the page stays in front afterwards
  const options: DisplayMediaStreamOptions & Record<string, unknown> = { video: { frameRate: { ideal: 15 }, displaySurface: 'window' }, audio: false, selfBrowserSurface: 'exclude', surfaceSwitching: 'include', monitorTypeSurfaces: 'include' }
  try {
    const Controller = (window as unknown as { CaptureController?: new () => { setFocusBehavior(to: string): void } }).CaptureController
    if (Controller) {
      const controller = new Controller()
      controller.setFocusBehavior('no-focus-change')
      options.controller = controller
    }
  } catch {
    /* the browser then decides which window is in front */
  }
  let stream: MediaStream
  try {
    stream = await navigator.mediaDevices.getDisplayMedia(options)
  } catch (e) {
    // closing the dialog is not an error worth words
    if (!(e instanceof DOMException && e.name === 'NotAllowedError')) status([['Screen not shared', e instanceof Error ? e.message : String(e)]])
    return
  }
  const track = stream.getVideoTracks()[0]
  const surface = { monitor: 'screen', window: 'window', browser: 'browser tab' }[String(track?.getSettings().displaySurface)] ?? 'screen'
  const ticker = new Worker(URL.createObjectURL(new Blob([`setInterval(() => postMessage(0), ${TICK})`], { type: 'text/javascript' })))
  sharing = { stream, tap: null, newest: null, ticker, surface }
  const video = $<HTMLVideoElement>('live-view')
  video.srcObject = stream
  void video.play().catch(() => undefined)
  tap(sharing)
  track?.addEventListener('ended', stop)
  // a timer in the page slows to a crawl once the game covers it; one in a worker does not
  ticker.addEventListener('message', () => void tick())
  showSharing()
  watch(watchingWords())
}

/** Offers screen sharing where the page runs in a browser that has it. */
export function startLive(): void {
  if (!canShare) return
  const button = $<HTMLButtonElement>('share')
  button.hidden = false
  $<HTMLElement>('way-share').hidden = false
  button.addEventListener('click', () => {
    if (sharing) void read(true).catch(e => say(`The shared picture could not be read: ${e instanceof Error ? e.message : String(e)}`))
    else void share()
  })
  $<HTMLButtonElement>('live-stop').addEventListener('click', stop)
  $<HTMLInputElement>('live-auto').addEventListener('change', () => {
    if (!live) watch(watchingWords())
  })
  $<HTMLButtonElement>('live-float').addEventListener('click', () => {
    if (isFloating()) closeFloat()
    else void openFloat(showSharing).then(showSharing)
    showSharing()
  })
  showSharing()
}
