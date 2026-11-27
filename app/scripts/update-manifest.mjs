// Writes the manifest installed copies read to find an update: node update-manifest.mjs <tag> <installer> <out file>
import { readFileSync, writeFileSync } from 'node:fs'
import { basename } from 'node:path'

const [tag, installer, out] = process.argv.slice(2)
const repository = process.env.GITHUB_REPOSITORY
if (!tag || !installer || !out || !repository) {
  console.error('usage: GITHUB_REPOSITORY=<owner/name> node update-manifest.mjs <tag> <installer> <out file>')
  process.exit(1)
}

const version = tag.replace(/^v/, '')
const built = JSON.parse(readFileSync(new URL('../src-tauri/tauri.conf.json', import.meta.url), 'utf8')).version
if (version !== built) {
  console.error(`tag ${tag} does not match the built version ${built}`)
  process.exit(1)
}

// the first section of the release notes says what is new in this version
const notes = readFileSync(new URL('../../docs/RELEASE-NOTES.md', import.meta.url), 'utf8')
const section = notes.split(/^## /m)[1] ?? ''
const manifest = {
  version,
  notes: section.split('\n').slice(1).join('\n').trim(),
  pub_date: new Date().toISOString(),
  platforms: {
    'windows-x86_64': {
      signature: readFileSync(`${installer}.sig`, 'utf8').trim(),
      url: `https://github.com/${repository}/releases/download/${tag}/${basename(installer)}`,
    },
  },
}
writeFileSync(out, JSON.stringify(manifest, null, 2) + '\n')
console.log(`${out}: ${version} -> ${manifest.platforms['windows-x86_64'].url}`)
