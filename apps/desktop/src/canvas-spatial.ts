export type Box = { x: number; y: number; width: number; height: number }
export function intersects(a: Box, b: Box) { return a.x <= b.x + b.width && a.x + a.width >= b.x && a.y <= b.y + b.height && a.y + a.height >= b.y }
function valid(box: Box) { return [box.x, box.y, box.width, box.height, box.x + box.width, box.y + box.height].every(Number.isFinite) && box.width >= 0 && box.height >= 0 }

/** Conservative grid index; oversized frames are kept outside the grid. */
export class CanvasSpatialIndex<T extends Box> {
  private readonly buckets = new Map<string, number[]>()
  private readonly oversized: number[] = []
  private readonly nodes: T[]
  constructor(nodes: T[]) {
    this.nodes = nodes
    nodes.forEach((node, index) => {
      if (!valid(node)) return
      const [left, top, right, bottom] = this.cells(node)
      if (![left, top, right, bottom].every(Number.isSafeInteger) || (right - left + 1) * (bottom - top + 1) > 64) { this.oversized.push(index); return }
      for (let x = left; x <= right; x++) for (let y = top; y <= bottom; y++) {
        const key = `${x}:${y}`
        const bucket = this.buckets.get(key)
        if (bucket) bucket.push(index)
        else this.buckets.set(key, [index])
      }
    })
  }
  private cells(box: Box) { return [Math.floor(box.x / 512), Math.floor(box.y / 512), Math.floor((box.x + box.width) / 512), Math.floor((box.y + box.height) / 512)] }
  query(box: Box) {
    if (!valid(box)) return []
    const [left, top, right, bottom] = this.cells(box)
    if (![left, top, right, bottom].every(Number.isSafeInteger) || (right - left + 1) * (bottom - top + 1) > 100_000) return this.nodes.filter((node) => valid(node) && intersects(node, box))
    const indices = new Set(this.oversized)
    for (let x = left; x <= right; x++) for (let y = top; y <= bottom; y++) for (const index of this.buckets.get(`${x}:${y}`) ?? []) indices.add(index)
    return [...indices].sort((a, b) => a - b).map((index) => this.nodes[index]).filter((node) => intersects(node, box))
  }
}
