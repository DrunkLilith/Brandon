import assert from 'node:assert/strict'
import test from 'node:test'
import { createCommandRegistry, bindingError, shortcutFromEvent, shortcutLabel, validShortcut } from '../../apps/desktop/src/commands.ts'
import { defaultSettings, parseSettings } from '../../apps/desktop/src/settings.ts'
import { anchorLine, sourceStructure, resolveNoteTarget, splitNoteTarget, linkAtPosition } from '../../apps/desktop/src/note-navigation.ts'

const event = (code, modifiers = {}) => ({ code, key: 'ы', ctrlKey: true, metaKey: false, altKey: false, shiftKey: false, repeat: false, isComposing: false, ...modifiers })
test('command registry is shared, detects duplicate IDs, and refuses unavailable commands', () => {
  let called = 0
  const command = { id: 'save', label: 'Сохранить', defaultShortcut: 'Mod+KeyS', run: () => { called++ } }
  const registry = createCommandRegistry([command, { ...command, id: 'denied', enabled: false }])
  assert.equal(registry.execute('save'), true); assert.equal(called, 1)
  assert.equal(registry.execute('denied'), false); assert.equal(registry.execute('missing'), false); assert.equal(called, 1)
  assert.throws(() => createCommandRegistry([command, command]), /Duplicate command/)
})
test('physical shortcuts work in RU/EN and reject IME, modifier-only and AltGraph input', () => {
  assert.equal(shortcutFromEvent(event('KeyS')), 'Mod+KeyS')
  assert.equal(shortcutFromEvent(event('KeyS', { key: 's' })), 'Mod+KeyS')
  assert.equal(shortcutFromEvent(event('KeyS', { isComposing: true })), null)
  assert.equal(shortcutFromEvent(event('ControlLeft')), null)
  assert.equal(shortcutFromEvent(event('KeyS', { getModifierState: key => key === 'AltGraph' })), null)
  assert.equal(shortcutLabel('Mod+Shift+Comma'), 'Ctrl + Shift + ,')
  assert.equal(validShortcut('KeyS'), false)
})
const commands = [
  { id: 'save', label: 'Сохранить', defaultShortcut: 'Mod+KeyS', run() {} },
  { id: 'open', label: 'Открыть', defaultShortcut: 'Mod+KeyO', run() {} },
  { id: 'undo', label: 'Отменить', defaultShortcut: 'Mod+KeyZ', run() {} },
]
test('custom bindings replace defaults, can be removed, and conflict checks include unavailable commands', () => {
  const registry = createCommandRegistry(commands, { save: 'Mod+Shift+KeyD', open: null })
  assert.equal(registry.forEvent(event('KeyS')), undefined)
  assert.equal(registry.forEvent(event('KeyD', { shiftKey: true })).id, 'save')
  assert.equal(registry.forEvent(event('KeyO')), undefined)
  assert.match(bindingError('save', 'Mod+KeyO', commands, {}), /Открыть/)
  assert.match(bindingError('save', 'Mod+KeyC', commands, {}), /зарезервировано/)
  assert.equal(bindingError('undo', 'Mod+KeyZ', commands, {}), null)
  assert.equal(bindingError('save', null, commands, {}), null)
})
test('malformed/imported duplicate or reserved bindings fail closed; Ctrl+plus has a physical-key alias', () => {
  assert.equal(createCommandRegistry(commands, { save: 'Mod+KeyC' }).forEvent(event('KeyC')), undefined)
  assert.equal(createCommandRegistry(commands, { save: 'KeyS' }).forEvent(event('KeyS', { ctrlKey: false })), undefined)
  assert.equal(createCommandRegistry(commands, { save: 'Mod+KeyO' }).forEvent(event('KeyO')), undefined)
  const registry = createCommandRegistry([{ id: 'zoom', label: 'Zoom', defaultShortcut: 'Mod+Equal', run() {} }])
  assert.equal(registry.forEvent(event('Equal', { shiftKey: true })).id, 'zoom')
})
test('settings are versioned, validated, safe on corruption, and migrate established visual preference', () => {
  assert.deepEqual(parseSettings('not json'), defaultSettings())
  assert.deepEqual(parseSettings('{"version":2,"lineNumbers":true}'), defaultSettings())
  const restored = parseSettings(JSON.stringify({ version: 1, editorMode: 'source', defaultViewMode: 'split', readableLineWidth: false, lineNumbers: true, spellcheck: false, previewModifier: 'none', hotkeys: { save: 'Mod+Shift+KeyD', open: null }, unknown: 1 }))
  assert.equal(restored.editorMode, 'source'); assert.equal(restored.defaultViewMode, 'split')
  assert.equal(restored.readableLineWidth, false); assert.equal(restored.lineNumbers, true)
  assert.equal(restored.spellcheck, false); assert.equal(restored.previewModifier, 'none')
  assert.deepEqual(restored.hotkeys, { save: 'Mod+Shift+KeyD', open: null })
  assert.equal(parseSettings(JSON.stringify(restored), 'visual').editorMode, 'visual')
  assert.equal(parseSettings('{"version":1,"lineNumbers":"yes","hotkeys":[]}').lineNumbers, false)
})
test('live outline has source lines, duplicate slugs, setext headings and no code/YAML headings', () => {
  const source = '---\ntitle: Name\n---\n# **Раздел**\n\n## Раздел\n\n## Раздел\n\nПодзаголовок\n---\n\n```md\n# Not a heading\n^not-a-block\n```\n\nText ^real\n'
  const structure = sourceStructure(source)
  assert.deepEqual(structure.headings.map(heading => [heading.text, heading.slug, heading.line]), [
    ['Раздел', 'раздел', 4], ['Раздел', 'раздел-1', 6], ['Раздел', 'раздел-2', 8], ['Подзаголовок', 'подзаголовок', 10],
  ])
  assert.deepEqual(structure.blocks, [{ id: 'real', line: 18 }])
  assert.equal(anchorLine(source, { heading: 'раздел-2' }), 8)
  assert.equal(anchorLine(source, { block: 'real' }), 18)
  assert.equal(anchorLine(source, { heading: 'missing' }), null)
})
test('outline preserves literal underscores and formatted inline code/link labels', () => {
  const headings = sourceStructure('# one_two **bold** `a_b` [label](Target.md)\n').headings
  assert.equal(headings[0].text, 'one_two bold a_b label')
})
test('heading IDs are unique even when a literal suffix collides with a repeated heading', () => {
  const headings = sourceStructure('# A\n\n# A-1\n\n# A\n\n# Number &#49;\n').headings
  assert.deepEqual(headings.map(heading => heading.slug), ['a', 'a-1', 'a-2', 'number-1'])
})
const docs = ['Root.md', 'Folder/Current.md', 'Folder/Local.md', 'Other/Local.md', 'Other/Другая заметка.md'].map(path => ({ path, title: path.split('/').at(-1).slice(0, -3), aliases: path === 'Root.md' ? ['Главная'] : [] }))
test('wiki and Markdown links retain heading/block targets, Unicode and same-file anchors', () => {
  assert.deepEqual(splitNoteTarget('Root#^block'), { path: 'Root', heading: null, block: 'block' })
  assert.equal(resolveNoteTarget('[[oops]]', 'Folder/Current.md', docs).status, 'missing')
  assert.equal(resolveNoteTarget('Local#Part', 'Folder/Current.md', docs).path, 'Folder/Local.md')
  assert.equal(resolveNoteTarget('Главная#Overview', 'Folder/Current.md', docs).path, 'Root.md')
  assert.equal(resolveNoteTarget('../Other/%D0%94%D1%80%D1%83%D0%B3%D0%B0%D1%8F%20%D0%B7%D0%B0%D0%BC%D0%B5%D1%82%D0%BA%D0%B0.md#^id', 'Folder/Current.md', docs, 'markdown').block, 'id')
  assert.equal(resolveNoteTarget('#^self', 'Folder/Current.md', docs).path, 'Folder/Current.md')
  assert.equal(resolveNoteTarget('/Root.md#Overview', 'Folder/Current.md', docs, 'markdown').path, 'Root.md')
})
test('ambiguous paths, traversal and unsafe URI never choose or create an arbitrary note', () => {
  assert.equal(resolveNoteTarget('Local', 'Root.md', docs).status, 'ambiguous')
  assert.equal(resolveNoteTarget('../../Root.md', 'Folder/Current.md', docs, 'markdown').status, 'missing')
  for (const target of ['javascript:alert(1)', 'file:///tmp/note', '//server/note', 'C:\\notes.md', 'a\0b']) assert.equal(resolveNoteTarget(target, 'Root.md', docs).status, 'invalid')
  assert.equal(resolveNoteTarget('%ZZ', 'Root.md', docs, 'markdown').status, 'invalid')
})
test('links at cursor exclude inline and fenced code and keep aliases out of destinations', () => {
  const source = 'Read [[Root#Overview|home]] and [local](Local.md#^block).\n`[[Root]]`\n```md\n[[Root]]\n```\n'
  assert.equal(linkAtPosition(source, source.indexOf('home')).target, 'Root#Overview')
  assert.equal(linkAtPosition(source, source.indexOf('local')).target, 'Local.md#^block')
  assert.equal(linkAtPosition(source, source.indexOf('Root', source.indexOf('`'))), null)
  assert.equal(linkAtPosition(source, source.lastIndexOf('Root')), null)
})
