import '@fontsource/rajdhani/500.css'
import '@fontsource/rajdhani/600.css'
import '@fontsource/rajdhani/700.css'
import '@fontsource/chakra-petch/500.css'
import '@fontsource/chakra-petch/600.css'
import '@fontsource/chakra-petch/700.css'
import './styles.css'

import { wireInput, wireSettings } from './controls'
import { warm } from './core'
import { desktop } from './desktop'
import { $ } from './dom'
import { drawGhost } from './draw'
import { status } from './render'
import { listSamples, loadSample } from './samples'
import { previewShell, startShell } from './shell'
import { state } from './state'

async function start(): Promise<void> {
  warm()
  wireInput()
  wireSettings()
  drawGhost($<SVGSVGElement>('ghost'))
  status([['Status', 'No frame yet']])
  void listSamples()
  await startShell()
  const params = new URLSearchParams(location.search)
  if (!desktop && params.get('shell') === 'preview') previewShell()
  const sample = params.get('sample')
  if (params.get('view') === 'screen') state.view = 'screen'
  if (sample) await loadSample(sample, sample)
}

void start()
