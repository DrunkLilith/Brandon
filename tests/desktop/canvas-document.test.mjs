import assert from 'node:assert/strict'
import test from 'node:test'
import { canvasColors, parseCanvasDocument } from '../../apps/desktop/src/canvas-document.ts'

const node = { id: 'one', type: 'text', x: 0, y: 0, width: 180, height: 100, text: 'Привет', custom: { keep: true } }
test('Canvas preserves unknown fields without laundering malformed documents into editable boards', () => {
  const input = { nodes: [node], edges: [], extension: { keep: true } }
  assert.deepEqual(parseCanvasDocument(JSON.stringify(input)), { document: input, error: null })
  for (const value of ['invalid', 'null', '[]', '{}', JSON.stringify({ nodes: [node, node], edges: [] }), JSON.stringify({ nodes: [{ ...node, width: -1 }], edges: [] }), JSON.stringify({ nodes: [{ ...node, text: 42 }], edges: [] })]) assert.ok(parseCanvasDocument(value).error)
})
test('Colors use dark fills and distinct borders, with safe fallback', () => {
  assert.deepEqual(canvasColors({ type: 'text', color: '6' }), { fill: '#322b3e', border: '#9381ba' })
  assert.equal(canvasColors({ type: 'text', color: '#FF0000' }).border, '#FF0000')
  assert.deepEqual(canvasColors({ type: 'text', color: 'invalid' }), canvasColors({ type: 'text' }))
})
