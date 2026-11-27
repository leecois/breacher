// The click-through window laid over the game: draws the route in the frame's own pixel space.
import '@fontsource/rajdhani/700.css'
import './styles.css'
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
  const caption = `${p.route.steps.length} of ${plan.buffer} picks, ${p.route.uploaded.length} of ${reading.sequences.length} daemons`
  drawOverlay(svg, reading, p.route, p.frame, { caption })
}

/** Tells a player inside the game why a capture came to nothing. */
function say(text: string): void {
  svg.replaceChildren()
  notice.textContent = text
  notice.hidden = false
}

if ('__TAURI_INTERNALS__' in window) {
  void import('@tauri-apps/api/event').then(({ listen }) => {
    void listen<OverlayPayload>('route', e => paint(e.payload))
    void listen<string>('notice', e => say(e.payload))
    void listen('clear', () => {
      svg.replaceChildren()
      notice.hidden = true
    })
  })
} else {
  // outside the shell a route or a notice can be posted to the page, to check the drawing
  window.addEventListener('message', e => (typeof e.data === 'string' ? say(e.data) : paint(e.data as OverlayPayload)))
}
