export function renderFixture(count) {
  const columns = count <= 12 ? 3 : Math.ceil(Math.sqrt(count))
  const rows = Math.ceil(count / columns)
  const nodes = Array.from({ length: count }, (_, index) => ({
    id: `fixture-${index}`, type: 'text', x: (index % columns - (columns - 1) / 2) * 220 - 90,
    y: (Math.floor(index / columns) - (rows - 1) / 2) * 132 - 48, width: 180, height: 96,
    text: index === 0 ? 'Привет, GPU!\nРусский текст · 123' : `Карточка ${index + 1}\nИдея → заметка → задача`,
    color: index === 0 ? '6' : undefined, extensionFixture: { preserved: true },
  }))
  const edges = nodes.flatMap((node, index) => index % columns !== columns - 1 && index + 1 < count
    ? [{ id: `edge-${index}`, fromNode: node.id, toNode: nodes[index + 1].id, extensionFixture: 'edge-kept' }] : [])
  return { nodes, edges, brandonFixture: true }
}
