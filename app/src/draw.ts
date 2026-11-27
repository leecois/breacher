// Two renderings of a route: drawSheet is the schematic for the main window, drawOverlay the route alone in frame pixels.
import { CODES, type Reading, type Route } from './types'

const NS = 'http://www.w3.org/2000/svg'

type Attrs = Record<string, string | number>

function el<K extends keyof SVGElementTagNameMap>(name: K, attrs: Attrs = {}, text?: string): SVGElementTagNameMap[K] {
  const node = document.createElementNS(NS, name)
  for (const [k, v] of Object.entries(attrs)) node.setAttribute(k, String(v))
  if (text !== undefined) node.textContent = text
  return node
}

/** Rows are lettered and columns numbered, like the zones along the edge of a drawing. */
export const rowName = (row: number): string => String.fromCharCode(65 + row)
export const cellName = (row: number, col: number): string => `${rowName(row)}${col + 1}`

function routePath(points: Array<[number, number]>): string {
  return points.map(([x, y], i) => `${i === 0 ? 'M' : 'L'}${x.toFixed(1)} ${y.toFixed(1)}`).join(' ')
}

/** A square with two opposite corners cut off, centred on the origin: the shape language of the UI. */
function cutSquare(half: number, cut: number): string {
  const h = half
  return `M${-h} ${-h}H${h - cut}L${h} ${-h + cut}V${h}H${-h + cut}L${-h} ${h - cut}Z`
}

/** A rectangle with the same two corners cut. */
function cutRect(x: number, y: number, w: number, h: number, cut: number): string {
  return `M${x} ${y}H${x + w - cut}L${x + w} ${y + cut}V${y + h}H${x + cut}L${x} ${y + h - cut}Z`
}

/** A chevron on each leg, half a cell before the destination: always a cell boundary, so it never covers a code. */
function chevrons(points: Array<[number, number]>, size: number, cls: string, halfCell: (horizontal: boolean) => number): SVGGElement {
  const g = el('g', { class: cls })
  for (let i = 0; i + 1 < points.length; i++) {
    const [ax, ay] = points[i]
    const [bx, by] = points[i + 1]
    const len = Math.hypot(bx - ax, by - ay)
    if (len < 1) continue
    const [ux, uy] = [(bx - ax) / len, (by - ay) / len]
    const back = halfCell(Math.abs(ux) > Math.abs(uy))
    const [mx, my] = [bx - ux * back, by - uy * back]
    const tip: [number, number] = [mx + ux * size * 0.6, my + uy * size * 0.6]
    const l: [number, number] = [mx - ux * size * 0.6 - uy * size * 0.7, my - uy * size * 0.6 + ux * size * 0.7]
    const r: [number, number] = [mx - ux * size * 0.6 + uy * size * 0.7, my - uy * size * 0.6 - ux * size * 0.7]
    g.append(el('polyline', { points: `${l[0].toFixed(1)},${l[1].toFixed(1)} ${tip[0].toFixed(1)},${tip[1].toFixed(1)} ${r[0].toFixed(1)},${r[1].toFixed(1)}` }))
  }
  return g
}

export interface SheetOptions {
  /** Play the one-off "marker draws the route" animation. */
  animate: boolean
}

/** The plan sheet: the matrix as a schematic, the route marked over it. */
export function drawSheet(svg: SVGSVGElement, reading: Pick<Reading, 'n' | 'matrix'>, route: Route, opts: SheetOptions): void {
  const n = reading.n
  const cell = 76
  const gutter = 38
  const pad = 12
  const size = gutter + n * cell + pad
  svg.replaceChildren()
  svg.setAttribute('viewBox', `0 0 ${size} ${size}`)
  svg.setAttribute('role', 'img')
  const order = route.steps.map(s => `${cellName(s.row, s.col)} ${CODES[s.code]}`).join(', then ')
  svg.setAttribute('aria-label', route.steps.length ? `Code matrix ${n} by ${n}. Pick ${order}.` : `Code matrix ${n} by ${n}. No route.`)

  const cx = (col: number): number => gutter + col * cell + cell / 2
  const cy = (row: number): number => gutter + row * cell + cell / 2

  // the first pick always comes from the top row: mark it the way the game marks the live line
  svg.append(el('rect', { class: 'sheet-start', x: gutter, y: gutter + 6, width: n * cell, height: cell - 12 }))

  // register marks at the cell corners, zone labels along the edges
  const marks = el('g', { class: 'sheet-marks' })
  for (let r = 0; r <= n; r++) {
    for (let c = 0; c <= n; c++) {
      const [x, y] = [gutter + c * cell, gutter + r * cell]
      marks.append(el('path', { d: `M${x - 4} ${y}H${x + 4}M${x} ${y - 4}V${y + 4}` }))
    }
  }
  svg.append(marks)
  const zones = el('g', { class: 'sheet-zones' })
  for (let i = 0; i < n; i++) {
    zones.append(el('text', { x: cx(i), y: gutter - 14, 'text-anchor': 'middle' }, String(i + 1)))
    zones.append(el('text', { x: gutter - 18, y: cy(i) + 5, 'text-anchor': 'middle' }, rowName(i)))
  }
  svg.append(zones)

  const pts: Array<[number, number]> = route.steps.map(s => [cx(s.col), cy(s.row)])
  if (pts.length) {
    const marker = el('path', { class: 'marker', d: routePath(pts), 'stroke-width': cell * 0.52, pathLength: 1 })
    if (opts.animate) marker.classList.add('draw')
    svg.append(marker)
    svg.append(chevrons(pts, 7, 'marker-chevrons', () => cell / 2))
  }

  const stepAt = new Map(route.steps.map((s, i) => [s.row * n + s.col, i]))
  const codes = el('g', { class: 'sheet-codes' })
  for (let r = 0; r < n; r++) {
    for (let c = 0; c < n; c++) {
      const t = el('text', { x: cx(c), y: cy(r) + 10, 'text-anchor': 'middle' }, CODES[reading.matrix[r * n + c]] ?? '??')
      if (stepAt.has(r * n + c)) t.classList.add('on')
      codes.append(t)
    }
  }
  svg.append(codes)

  const badges = el('g', { class: 'badges' })
  route.steps.forEach((s, i) => {
    const [bx, by] = [cx(s.col) - cell * 0.34, cy(s.row) - cell * 0.31]
    const g = el('g', { class: 'badge', transform: `translate(${bx} ${by})` })
    if (opts.animate) {
      g.classList.add('pop')
      g.style.animationDelay = `${180 + i * 70}ms`
    }
    g.append(el('path', { d: cutSquare(11.5, 6) }))
    g.append(el('text', { y: 5, 'text-anchor': 'middle' }, String(i + 1)))
    badges.append(g)
  })
  svg.append(badges)
}

/** A stand-in result for the empty state: shows what the tool produces before any screen is read. */
export function drawGhost(svg: SVGSVGElement): void {
  const n = 6
  // fixed, so the picture is the same every time
  const matrix = [4, 0, 4, 3, 0, 4, 0, 2, 0, 1, 3, 1, 2, 0, 0, 1, 1, 0, 0, 3, 0, 1, 1, 0, 0, 1, 2, 1, 0, 0, 3, 3, 0, 0, 1, 2]
  const cells: Array<[number, number]> = [[0, 1], [1, 1], [1, 3], [0, 3], [0, 4], [2, 4]]
  const route: Route = { steps: cells.map(([row, col]) => ({ row, col, code: matrix[row * n + col], x: 0, y: 0, uploads: [] })), uploaded: [], missed: [] }
  drawSheet(svg, { n, matrix }, route, { animate: false })
  svg.removeAttribute('role')
  svg.removeAttribute('aria-label')
}

export interface OverlayOptions {
  /** Short line shown under the matrix, e.g. "6 picks, 2 of 3 daemons". */
  caption?: string
  /** Where the frame sits inside the element: centred (overlay window) or top-left (over an <img>). */
  align?: 'center' | 'start'
}

/** The route alone, drawn in the pixel space of the captured frame. */
export function drawOverlay(svg: SVGSVGElement, reading: Reading, route: Route, frame: { width: number; height: number }, opts: OverlayOptions = {}): void {
  const g = reading.geometry
  svg.replaceChildren()
  svg.setAttribute('viewBox', `0 0 ${frame.width} ${frame.height}`)
  svg.setAttribute('preserveAspectRatio', opts.align === 'start' ? 'xMinYMin meet' : 'xMidYMid meet')
  const unit = Math.min(g.px, g.py) / 64 // 1.0 at 1080p
  const pts: Array<[number, number]> = route.steps.map(s => [s.x, s.y])
  if (!pts.length) return

  svg.append(el('path', { class: 'ov-marker', d: routePath(pts), 'stroke-width': g.py * 0.6 }))
  svg.append(chevrons(pts, 9 * unit, 'ov-chevrons', horizontal => (horizontal ? g.px : g.py) / 2))
  const [rw, rh] = [g.px * 0.86, g.py * 0.7]
  route.steps.forEach((s, i) => {
    svg.append(el('path', { class: 'ov-ring', d: cutRect(s.x - rw / 2, s.y - rh / 2, rw, rh, 9 * unit), 'stroke-width': 3 * unit }))
    const [bx, by] = [s.x - rw / 2, s.y - rh / 2]
    const badge = el('g', { class: 'ov-badge', transform: `translate(${bx} ${by})` })
    badge.append(el('path', { d: cutSquare(13 * unit, 6.5 * unit), 'stroke-width': 2.5 * unit }))
    badge.append(el('text', { y: 5.4 * unit, 'text-anchor': 'middle', 'font-size': 16 * unit }, String(i + 1)))
    svg.append(badge)
  })
  if (opts.caption) {
    const x = g.x0 - g.px * 0.43
    const y = g.y0 + (g.n - 0.5) * g.py + 30 * unit
    const text = el('text', { class: 'ov-caption', x: x + 12 * unit, y: y + 19.5 * unit, 'font-size': 16 * unit }, opts.caption)
    svg.append(text)
    // background sized to the text once it is in the document
    queueMicrotask(() => {
      const box = text.getBBox()
      const bg = el('path', { class: 'ov-caption-bg', d: cutRect(x, y, box.width + 24 * unit, 28 * unit, 8 * unit) })
      svg.insertBefore(bg, text)
    })
  }
}
