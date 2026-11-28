// The page in the player's language. English is the source: its sentences are the keys, and a sentence without a translation stays as it is.
import { patterns, words } from './vi'

export type Lang = 'en' | 'vi'

const KEY = 'breacher.lang'
const NAMES: Record<Lang, string> = { en: 'English', vi: 'Tiếng Việt' }

function chosen(): Lang {
  const stored = localStorage.getItem(KEY)
  if (stored === 'en' || stored === 'vi') return stored
  return navigator.language.toLowerCase().startsWith('vi') ? 'vi' : 'en'
}

export let lang: Lang = chosen()

const fill = (text: string, values: Record<string, string | number>): string => text.replace(/\{(\w+)\}/g, (whole, name: string) => (name in values ? String(values[name]) : whole))

/** A sentence in the player's language; `{name}` in it stands for a value. */
export function t(text: string, values: Record<string, string | number> = {}): string {
  return fill(lang === 'vi' ? (words[text] ?? text) : text, values)
}

/** A sentence that was put together elsewhere, such as a hint from the reader or the desktop shell: translated whole, or by the shape it has. */
export function tWhole(text: string): string {
  if (lang !== 'vi') return text
  const whole = words[text]
  if (whole !== undefined) return whole
  for (const [shape, to] of patterns) {
    const found = shape.exec(text)
    if (found) return to.replace(/\$(\d)/g, (_, i: string) => tWhole(found[Number(i)] ?? ''))
  }
  return text
}

const ATTRIBUTES = ['aria-label', 'title', 'alt', 'placeholder']

/** Puts the text that was written into the page itself into the player's language. */
export function translatePage(root: ParentNode = document.body): void {
  if (lang === 'en') return
  document.documentElement.lang = lang
  const walker = document.createTreeWalker(root, NodeFilter.SHOW_TEXT)
  for (let node = walker.nextNode(); node; node = walker.nextNode()) {
    const text = node.nodeValue ?? ''
    const said = text.trim()
    if (said && words[said] !== undefined) node.nodeValue = text.replace(said, words[said])
  }
  for (const element of root.querySelectorAll<HTMLElement>('*')) {
    for (const name of ATTRIBUTES) {
      const value = element.getAttribute(name)
      if (value && words[value] !== undefined) element.setAttribute(name, words[value])
    }
  }
}

/** Follows a change of language made in another window of the program. */
export function followLanguage(changed: () => void): void {
  window.addEventListener('storage', e => {
    if (e.key !== KEY) return
    lang = chosen()
    changed()
  })
}

/** A list to choose the language from; choosing loads the page afresh in that language. */
export function languageChoice(): HTMLSelectElement {
  const select = document.createElement('select')
  select.id = 'lang'
  select.setAttribute('aria-label', t('Language'))
  for (const code of Object.keys(NAMES) as Lang[]) {
    const option = document.createElement('option')
    option.value = code
    option.textContent = NAMES[code]
    select.append(option)
  }
  select.value = lang
  select.addEventListener('change', () => {
    localStorage.setItem(KEY, select.value)
    location.reload()
  })
  return select
}
