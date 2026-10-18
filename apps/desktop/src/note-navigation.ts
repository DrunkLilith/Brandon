import { markdownLanguage } from '@codemirror/lang-markdown'
import type { DocumentMetadata } from './types'

export type NoteLocation = { line?: number; heading?: string | null; block?: string | null }
export type SourceHeading = { level: number; text: string; slug: string; line: number }
export type NoteStructure = { headings: SourceHeading[]; blocks: { id: string; line: number }[] }

export function headingSlug(value: string): string {
  return value.trim().toLocaleLowerCase('ru').replace(/[^\p{L}\p{N}_-]+/gu, ' ').trim().replace(/\s+/g, '-')
}

type HeadingNode = { name: string; from: number; to: number; firstChild: HeadingNode | null; nextSibling: HeadingNode | null }
function plainHeading(node: HeadingNode, source: string): string {
  const omitted: { from: number; to: number }[] = []
  function visit(child: HeadingNode) {
    if (['HeaderMark', 'EmphasisMark', 'CodeMark', 'LinkMark', 'URL', 'LinkTitle', 'HTMLTag'].includes(child.name)) { omitted.push(child); return }
    for (let nested = child.firstChild; nested; nested = nested.nextSibling) visit(nested)
  }
  visit(node)
  let value = source.slice(node.from, node.to)
  for (const range of omitted.sort((left, right) => right.from - left.from)) value = value.slice(0, range.from - node.from) + value.slice(range.to - node.from)
  return value
    .replace(/\[\[([^\]|]+)(?:\|([^\]]+))?\]\]/g, (_, target, label) => label ?? target)
    .replace(/\\([\p{P}])/gu, '$1').replace(/&#(x[\da-f]+|\d+);/gi, (_, value: string) => {
      const code = value.toLowerCase().startsWith('x') ? parseInt(value.slice(1), 16) : Number(value)
      return code > 0 && code <= 0x10ffff && !(code >= 0xd800 && code <= 0xdfff) ? String.fromCodePoint(code) : '\ufffd'
    }).replace(/&(?:amp|lt|gt|quot|apos);/g, (value) => ({ '&amp;': '&', '&lt;': '<', '&gt;': '>', '&quot;': '"', '&apos;': "'" })[value] ?? value).trim()
}

export function sourceStructure(source: string): NoteStructure {
  const frontmatter = /^(?:\uFEFF)?---\r?\n[\s\S]*?\r?\n(?:---|\.\.\.)[\t ]*(?:\r?\n|$)/.exec(source)
  const masked = frontmatter ? frontmatter[0].replace(/[^\r\n]/g, ' ') + source.slice(frontmatter[0].length) : source
  const lines = [0]
  for (let position = 0; position < source.length; position++) if (source[position] === '\n') lines.push(position + 1)
  const lineAt = (position: number) => {
    let low = 0, high = lines.length
    while (low + 1 < high) { const middle = (low + high) >>> 1; if (lines[middle] <= position) low = middle; else high = middle }
    return low + 1
  }
  const headings: SourceHeading[] = [], blocks: NoteStructure['blocks'] = [], excluded: { from: number; to: number }[] = []
  const slugs = new Map<string, number>()
  const usedSlugs = new Set<string>()
  markdownLanguage.parser.parse(masked).iterate({ enter(node) {
    if (['FencedCode', 'CodeBlock', 'InlineCode'].includes(node.name)) { excluded.push({ from: node.from, to: node.to }); return false }
    const heading = /^(?:ATX|Setext)Heading([1-6])$/.exec(node.name)
    if (!heading) return
    const text = plainHeading(node.node, source), base = headingSlug(text)
    if (!text) return
    let occurrence = slugs.get(base) ?? 0
    let slug = occurrence ? `${base}-${occurrence}` : base
    while (usedSlugs.has(slug)) { occurrence++; slug = `${base}-${occurrence}` }
    slugs.set(base, occurrence + 1)
    usedSlugs.add(slug)
    headings.push({ level: Number(heading[1]), text, slug, line: lineAt(node.from) })
  } })
  for (const match of masked.matchAll(/(?:^|[\t ])\^([A-Za-z0-9-]+)[\t ]*$/gm)) {
    if (!excluded.some((range) => match.index >= range.from && match.index < range.to)) blocks.push({ id: match[1], line: lineAt(match.index) })
  }
  return { headings, blocks }
}

export function anchorLine(source: string, location: NoteLocation): number | null {
  if (location.line !== undefined) return Math.max(1, Math.min(Math.trunc(location.line), source.split('\n').length))
  const structure = sourceStructure(source)
  if (location.block) return structure.blocks.find((block) => block.id === location.block)?.line ?? null
  if (location.heading) {
    const name = location.heading.trim().toLocaleLowerCase('ru')
    return (structure.headings.find((heading) => heading.text.toLocaleLowerCase('ru') === name)
      ?? structure.headings.find((heading) => heading.slug === name || heading.slug === headingSlug(name)))?.line ?? null
  }
  return 1
}

export function splitNoteTarget(value: string, kind: 'wiki' | 'markdown' = 'wiki') {
  let destination = value.trim()
  if (kind === 'markdown') {
    try { destination = decodeURIComponent(destination) } catch { return null }
  }
  if (/^[a-z][a-z0-9+.-]*:/i.test(destination) || destination.startsWith('//') || destination.includes('\0') || destination.includes('\\')) return null
  const separator = destination.indexOf('#')
  const legacy = separator < 0 ? destination.indexOf('^') : -1
  const path = (separator >= 0 ? destination.slice(0, separator) : legacy >= 0 ? destination.slice(0, legacy) : destination).trim()
  const fragment = separator >= 0 ? destination.slice(separator + 1) : legacy >= 0 ? `^${destination.slice(legacy + 1)}` : ''
  return { path, heading: fragment && !fragment.startsWith('^') ? fragment : null, block: fragment.startsWith('^') ? fragment.slice(1) : null }
}

function normalizedPath(value: string): string | null {
  const parts: string[] = []
  for (const part of value.split('/')) {
    if (!part || part === '.') continue
    if (part === '..') { if (!parts.length) return null; parts.pop() } else parts.push(part)
  }
  return parts.join('/')
}

export function resolveNoteTarget(value: string, currentPath: string, documents: Pick<DocumentMetadata, 'path' | 'title' | 'aliases'>[], kind: 'wiki' | 'markdown' = 'wiki') {
  const target = splitNoteTarget(value, kind)
  if (!target) return { status: 'invalid' as const }
  if (!target.path) return { status: 'resolved' as const, ...target, path: currentPath }
  const folder = currentPath.includes('/') ? currentPath.slice(0, currentPath.lastIndexOf('/') + 1) : ''
  const normalize = (path: string) => path.replace(/\.md$/i, '').toLocaleLowerCase('ru')
  const relative = normalizedPath(`${folder}${target.path}`), root = normalizedPath(target.path)
  const candidates = target.path.startsWith('/') ? [root] : kind === 'markdown' ? [relative] : target.path.startsWith('./') || target.path.startsWith('../') ? [relative] : [relative, root]
  for (const candidate of candidates) {
    if (candidate === null) continue
    const exact = documents.filter((document) => normalize(document.path) === normalize(candidate))
    if (exact.length === 1) return { status: 'resolved' as const, ...target, path: exact[0].path }
    if (exact.length > 1) return { status: 'ambiguous' as const }
  }
  if (kind === 'wiki' && root !== null && !target.path.includes('/')) {
    const name = normalize(target.path)
    const aliases = documents.filter((document) => [document.path.split('/').at(-1) ?? '', document.title, ...document.aliases].some((alias) => normalize(alias) === name))
    if (aliases.length === 1) return { status: 'resolved' as const, ...target, path: aliases[0].path }
    if (aliases.length > 1) return { status: 'ambiguous' as const }
  }
  return { status: 'missing' as const }
}

export function linkAtPosition(source: string, position: number): { target: string; kind: 'wiki' | 'markdown'; from: number; to: number } | null {
  const tree = markdownLanguage.parser.parse(source)
  let excluded = false
  tree.iterate({ from: Math.max(0, position - 1), to: Math.min(source.length, position + 1), enter(node) {
    if (['FencedCode', 'CodeBlock', 'InlineCode'].includes(node.name)) { excluded = true; return false }
  } })
  if (excluded) return null
  const lineStart = source.lastIndexOf('\n', Math.max(0, position - 1)) + 1
  const lineEnd = source.indexOf('\n', position)
  const line = source.slice(lineStart, lineEnd < 0 ? undefined : lineEnd)
  for (const match of line.matchAll(/!?\[\[([^\]|]+)(?:\|[^\]]*)?\]\]|\[[^\]]*\]\(([^\s)]*)(?:\s+"[^"\n]*")?\)/g)) {
    const from = lineStart + match.index, to = from + match[0].length
    if (position >= from && position <= to) return { target: match[1] ?? match[2], kind: match[1] !== undefined ? 'wiki' : 'markdown', from, to }
  }
  return null
}
