import { t } from './i18n'
import type { Route } from './types'

/** One line on where a route stands: what it uploads, or which pick is next once picks have been seen. */
export function routeCaption(route: Route, buffer: number, daemons: number, check: boolean): string {
  const [done, picks] = [route.done ?? 0, route.steps.length]
  const uploaded = route.uploaded.length
  if (done >= picks && picks > 0) return t('All {n} picks made', { n: picks })
  if (done > 0) return check ? t('Rule check: pick {n} of {total}', { n: done + 1, total: picks }) : t('Pick {n} of {total}, {uploaded} of {daemons} daemons', { n: done + 1, total: picks, uploaded, daemons })
  return check ? t('Rule check: play exactly these {n} picks', { n: picks }) : t('{picks} of {buffer} picks, {uploaded} of {daemons} daemons', { picks, buffer, uploaded, daemons })
}
