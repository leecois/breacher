import type { Route } from './types'

/** One line on where a route stands: what it uploads, or which pick is next once picks have been seen. */
export function routeCaption(route: Route, buffer: number, daemons: number, check: boolean): string {
  const [done, picks] = [route.done ?? 0, route.steps.length]
  const uploads = `${route.uploaded.length} of ${daemons} daemons`
  if (done >= picks && picks > 0) return `All ${picks} picks made`
  if (done > 0) return check ? `Rule check: pick ${done + 1} of ${picks}` : `Pick ${done + 1} of ${picks}, ${uploads}`
  return check ? `Rule check: play exactly these ${picks} picks` : `${picks} of ${buffer} picks, ${uploads}`
}
