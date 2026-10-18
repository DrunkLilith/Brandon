import { LanguageDescription, syntaxHighlighting } from '@codemirror/language'
import { languages } from '@codemirror/language-data'
import { classHighlighter, highlightTree } from '@lezer/highlight'

export { languages as codeLanguages }
export const codeHighlighting = syntaxHighlighting(classHighlighter)

const aliases: Record<string, string> = { rs: 'rust', py: 'python', js: 'javascript', ts: 'typescript', sh: 'shell', bash: 'shell', cs: 'c#', csharp: 'c#', yml: 'yaml', ps1: 'powershell', ps: 'powershell' }

export function codeLanguage(name: string) {
  const normalized = name.trim().split(/\s+/, 1)[0]?.toLowerCase() ?? ''
  if (!normalized || ['text', 'txt', 'plain', 'plaintext'].includes(normalized)) return null
  return LanguageDescription.matchLanguageName(languages, aliases[normalized] ?? normalized, false)
}

export async function highlightedCode(code: string, name: string) {
  const language = codeLanguage(name)
  if (!language) return [{ text: code, className: '' }]
  const support = await language.load()
  const pieces: Array<{ text: string; className: string }> = []
  let position = 0
  highlightTree(support.language.parser.parse(code), classHighlighter, (from, to, className) => {
    if (from > position) pieces.push({ text: code.slice(position, from), className: '' })
    pieces.push({ text: code.slice(from, to), className })
    position = to
  })
  if (position < code.length) pieces.push({ text: code.slice(position), className: '' })
  return pieces
}

export function inlineCodeSource(value: string) {
  const code = value.replace(/\r?\n/g, ' ') || 'код'
  const fence = '`'.repeat(Math.max(1, ...(code.match(/`+/g) ?? []).map((run) => run.length + 1)))
  const padding = code.startsWith('`') || code.endsWith('`') || (/^ .* $/.test(code) && /\S/.test(code)) ? ' ' : ''
  return { value: `${fence}${padding}${code}${padding}${fence}`, offset: fence.length + padding.length, length: code.length }
}
