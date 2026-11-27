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
  const [done, picks] = [p.route.done ?? 0, p.route.steps.length]
  const daemons = `${p.route.uploaded.length} of ${reading.sequences.length} daemons`
  // once picks have been seen the line counts them off
  const caption =
    done >= picks && picks > 0
      ? `All ${picks} picks made`
      : done > 0
        ? p.check
          ? `Rule check: pick ${done + 1} of ${picks}`
          : `Pick ${done + 1} of ${picks}, ${daemons}`
        : p.check
          ? `Rule check: play exactly these ${picks} picks`
          : `${picks} of ${plan.buffer} picks, ${daemons}`
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
