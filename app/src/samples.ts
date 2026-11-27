import { $ } from './dom'
import { readBlob } from './session'

export async function listSamples(): Promise<void> {
  try {
    const res = await fetch(new URL('./samples/index.json', document.baseURI))
    if (!res.ok) return
    const names = (await res.json()) as Array<{ file: string; label: string }>
    const box = $<HTMLElement>('samples')
    for (const s of names) {
      const b = document.createElement('button')
      b.className = 'btn small'
      b.textContent = s.label
      b.addEventListener('click', () => void loadSample(s.file, s.label))
      box.append(b)
    }
    box.hidden = names.length === 0
  } catch {
    /* no samples bundled */
  }
}

export async function loadSample(file: string, label: string): Promise<void> {
  const res = await fetch(new URL(`./samples/${file}`, document.baseURI))
  if (res.ok) await readBlob(await res.blob(), label)
}
