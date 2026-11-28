// The click-through window laid over the game: draws the route in the frame's own pixel space.
import '@fontsource/rajdhani/700.css'
import './styles.css'
import { routeCaption } from './caption'
import { drawOverlay } from './draw'
import type { OverlayPayload } from './desktop'

const svg = document.getElementById('overlay') as unknown as SVGSVGElement
const notice = document.getElementById('notice') as HTMLElement

function paint(p: OverlayPayload): void {
  if (p.response.status !== 'ok' || !p.route) {
    svg.replaceChildren()
    return
  }
  notice.hidden = true
  const { reading, plan } = p.response
  const caption = routeCaption(p.route, plan.buffer, reading.sequences.length, Boolean(p.check))
  drawOverlay(svg, reading, p.route, p.frame, { caption })
}

/** Tells a player inside the game why a capture came to nothing. */
function say(text: string): void {
  svg.replaceChildren()
  notice.textContent = text
  notice.hidden = false
}

// the breach the shell has a route up for: the main window may redraw that one, but not put a route up on its own
let up: string | null = null

const breachOf = (p: OverlayPayload): string | null => (p.response.status === 'ok' && p.route ? JSON.stringify([p.response.reading.matrix, p.response.reading.sequences]) : null)

if ('__TAURI_INTERNALS__' in window) {
  void import('@tauri-apps/api/event').then(({ listen }) => {
    void listen<OverlayPayload>('route', e => {
      up = breachOf(e.payload)
      paint(e.payload)
    })
    void listen<OverlayPayload>('redraw', e => {
      if (up !== null && breachOf(e.payload) === up) paint(e.payload)
    })
    void listen<string>('notice', e => {
      up = null
      say(e.payload)
    })
    void listen('clear', () => {
      up = null
      svg.replaceChildren()
      notice.hidden = true
    })
  })
} else {
  // outside the shell a route or a notice can be posted to the page, to check the drawing
  window.addEventListener('message', e => (typeof e.data === 'string' ? say(e.data) : paint(e.data as OverlayPayload)))
}
