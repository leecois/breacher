// The click-through window laid over the game: draws the route in the frame's own pixel space.
import '@fontsource/rajdhani/700.css'
import './styles.css'
import { drawOverlay } from './draw'
import type { OverlayPayload } from './desktop'

const svg = document.getElementById('overlay') as unknown as SVGSVGElement

function paint(p: OverlayPayload): void {
  if (p.response.status !== 'ok' || !p.route) {
    svg.replaceChildren()
    return
  }
  const { reading, plan } = p.response
  const caption = `${p.route.steps.length} of ${plan.buffer} picks, ${p.route.uploaded.length} of ${reading.sequences.length} daemons`
  drawOverlay(svg, reading, p.route, p.frame, { caption })
}

if ('__TAURI_INTERNALS__' in window) {
  void import('@tauri-apps/api/event').then(({ listen }) => {
    void listen<OverlayPayload>('route', e => paint(e.payload))
    void listen('clear', () => svg.replaceChildren())
  })
} else {
  // outside the shell a route can be posted to the page, to check the drawing
  window.addEventListener('message', e => paint(e.data as OverlayPayload))
}
