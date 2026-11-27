// The route in a small window that stays above the others, for a page that cannot draw over the game.
import { routeCaption } from './caption'
import { drawSheet } from './draw'
import { shownRoute, state } from './state'

interface Floater {
  requestWindow(options: { width: number; height: number }): Promise<Window>
}

const floater = (window as unknown as { documentPictureInPicture?: Floater }).documentPictureInPicture

/** Whether this browser can keep a window of the page above the game. */
export const canFloat = Boolean(floater)

let floating: Window | null = null
let idle = ''

export const isFloating = (): boolean => floating !== null

/** Draws what the page shows into the floating window, when one is open; `waiting` is the line for when there is no route. */
export function paintFloat(waiting?: string): void {
  if (waiting !== undefined) idle = waiting
  if (!floating) return
  const line = floating.document.getElementById('float-line')
  const sheet = floating.document.getElementById('float-sheet') as SVGSVGElement | null
  if (!line || !sheet) return
  const { reading, plan } = state
  const route = shownRoute()
  if (reading && plan && route) {
    line.textContent = routeCaption(route, plan.buffer, state.wanted.filter(Boolean).length || reading.sequences.length, Boolean(state.check))
    drawSheet(sheet, reading, route, { animate: false })
  } else {
    line.textContent = idle
    sheet.replaceChildren()
  }
}

/** Opens the floating window; must be called from a click. `closed` hears when the player closes it. */
export async function openFloat(closed: () => void): Promise<void> {
  if (!floater || floating) return
  const win = await floater.requestWindow({ width: 400, height: 452 })
  const doc = win.document
  for (const sheet of [...document.styleSheets]) {
    // a linked sheet is linked again so that the fonts it names are found; an inline one is copied
    if (sheet.href) {
      const link = doc.createElement('link')
      link.rel = 'stylesheet'
      link.href = sheet.href
      doc.head.append(link)
      continue
    }
    try {
      const style = doc.createElement('style')
      style.textContent = [...sheet.cssRules].map(rule => rule.cssText).join('\n')
      doc.head.append(style)
    } catch {
      /* a sheet the page may not read is left out */
    }
  }
  doc.title = 'Breacher route'
  doc.body.className = 'floating'
  const line = doc.createElement('p')
  line.id = 'float-line'
  const sheet = doc.createElementNS('http://www.w3.org/2000/svg', 'svg')
  sheet.id = 'float-sheet'
  doc.body.append(line, sheet)
  win.addEventListener('pagehide', () => {
    floating = null
    closed()
  })
  floating = win
  paintFloat()
}

export function closeFloat(): void {
  floating?.close()
  floating = null
}
