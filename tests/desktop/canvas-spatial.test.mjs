import assert from 'node:assert/strict'
import test from 'node:test'
import { CanvasSpatialIndex, intersects } from '../../apps/desktop/src/canvas-spatial.ts'

test('10,000-node indexed queries match a full scan and preserve paint order', () => {
  let seed = 29
  const random = () => { seed = (Math.imul(seed, 1664525) + 1013904223) >>> 0; return seed / 2 ** 32 }
  const nodes = Array.from({ length: 10000 }, (_, id) => ({ id, x: random() * 40000 - 20000, y: random() * 40000 - 20000, width: random() * 500 + 1, height: random() * 300 + 1 }))
  nodes.push({ id: 10000, x: -100000, y: -100000, width: 200000, height: 200000 })
  const index = new CanvasSpatialIndex(nodes)
  for (let attempt = 0; attempt < 150; attempt++) {
    const box = { x: random() * 40000 - 20000, y: random() * 40000 - 20000, width: random() * 15000, height: random() * 15000 }
    assert.deepEqual(index.query(box), nodes.filter((node) => intersects(node, box)))
  }
})
test('point queries, negative coordinates, shared boundaries, and oversized objects', () => {
  const nodes = [{ x: -512, y: -512, width: 512, height: 512 }, { x: -1e6, y: -1e6, width: 2e6, height: 2e6 }]
  const index = new CanvasSpatialIndex(nodes)
  assert.deepEqual(index.query({ x: 0, y: 0, width: 0, height: 0 }), nodes)
  assert.deepEqual(index.query({ x: 10, y: 10, width: 1, height: 1 }), [nodes[1]])
})
test('malformed and enormous geometry cannot hang grid loops', () => {
  const huge = { x: 1e30, y: 1e30, width: 1e30, height: 1e30 }
  const index = new CanvasSpatialIndex([huge, { x: Infinity, y: 0, width: 10, height: 10 }, { x: NaN, y: 0, width: 0, height: 0 }])
  assert.deepEqual(index.query(huge), [huge])
  assert.deepEqual(index.query({ x: NaN, y: 0, width: 1, height: 1 }), [])
  assert.deepEqual(index.query({ x: 0, y: 0, width: Infinity, height: Infinity }), [])
})
