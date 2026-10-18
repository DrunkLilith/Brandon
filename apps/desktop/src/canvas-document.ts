import type { CanvasDocument, CanvasNode } from './canvas-view'

export function parseCanvasDocument(source: string): { document: CanvasDocument; error: string | null } {
  try {
    const value: unknown = JSON.parse(source)
    if (!value || typeof value !== 'object' || Array.isArray(value)) throw new Error('Ожидается объект JSON Canvas')
    const document = value as CanvasDocument
    if (!Array.isArray(document.nodes) || !Array.isArray(document.edges)) throw new Error('Нужны массивы nodes и edges')
    const ids = new Set<string>()
    for (const node of document.nodes) {
      if (!node || typeof node.id !== 'string' || !node.id || ids.has(node.id)) throw new Error('Пустой или повторяющийся ID карточки')
      ids.add(node.id)
      if (!['text', 'file', 'link', 'group'].includes(node.type)) throw new Error('Неизвестный тип карточки')
      if (![node.x, node.y, node.width, node.height].every((v) => typeof v === 'number' && Number.isFinite(v) && Math.abs(v) <= 10_000_000) || node.width <= 0 || node.height <= 0) throw new Error('Некорректные координаты или размер карточки')
      for (const key of ['text', 'file', 'url', 'label', 'color'] as const) if (node[key] !== undefined && typeof node[key] !== 'string') throw new Error(`Поле ${key} должно быть строкой`)
    }
    const edgeIds = new Set<string>()
    for (const edge of document.edges) {
      if (!edge || typeof edge.id !== 'string' || !edge.id || edgeIds.has(edge.id) || typeof edge.fromNode !== 'string' || typeof edge.toNode !== 'string') throw new Error('Некорректная связь или повторяющийся ID')
      edgeIds.add(edge.id)
    }
    return { document, error: null }
  } catch (error) {
    return { document: { nodes: [], edges: [] }, error: error instanceof Error ? error.message : 'Некорректный JSON Canvas' }
  }
}

export function canvasColors(node: Pick<CanvasNode, 'type' | 'color'>) {
  const palettes: Record<string, [string, string]> = {
    '1': ['#38272a', '#a96770'], '2': ['#392d25', '#b58259'], '3': ['#363225', '#a89a58'],
    '4': ['#26352c', '#649776'], '5': ['#253339', '#5e97a7'], '6': ['#322b3e', '#9381ba'],
  }
  if (node.color && palettes[node.color]) return { fill: palettes[node.color][0], border: palettes[node.color][1] }
  if (node.color && /^#[\da-f]{6}$/i.test(node.color)) {
    const value = Number.parseInt(node.color.slice(1), 16)
    const parts = [value >> 16, value >> 8 & 255, value & 255].map((channel) => Math.round(channel * .18 + 30 * .82).toString(16).padStart(2, '0'))
    return { fill: `#${parts.join('')}`, border: node.color }
  }
  return { fill: node.type === 'group' ? '#222128' : '#28282b', border: '#4a484f' }
}
