import { useEffect, useMemo, useRef, useState } from 'react'
import type { components } from '../api/generated'
import { useI18n } from '../i18n'
type Node = components['schemas']['PageTreeItem']
const rowHeight = 36,
  visibleCount = 14,
  overscan = 3

export function PageTree({
  nodes,
  selected,
  onSelect,
}: {
  nodes: readonly Node[]
  selected?: string
  onSelect(id: string): void
}) {
  const { messages: text } = useI18n()
  const [expanded, setExpanded] = useState<Set<string>>(() => {
    const ancestors = new Set<string>()
    let id = selected
    while (id && !ancestors.has(id)) {
      ancestors.add(id)
      id = nodes.find((n) => n.id === id)?.parentId ?? undefined
    }
    return ancestors
  })
  const [active, setActive] = useState(selected)
  const [start, setStart] = useState(0)
  const container = useRef<HTMLDivElement>(null)
  const previousSelection = useRef(selected)
  const scrolledSelection = useRef<string | undefined>(undefined)
  useEffect(() => {
    if (selected === previousSelection.current) return
    previousSelection.current = selected
    setActive(selected)
    const ancestors = new Set<string>()
    let id = selected
    while (id && !ancestors.has(id)) {
      ancestors.add(id)
      id = nodes.find((n) => n.id === id)?.parentId ?? undefined
    }
    setExpanded((old) => new Set([...old, ...ancestors]))
  }, [selected, nodes])
  const rows = useMemo(() => {
    const children = new Map<string | null, Node[]>()
    const ids = new Set(nodes.map((n) => n.id))
    for (const n of nodes) {
      const parent = n.parentId && ids.has(n.parentId) ? n.parentId : null
      children.set(parent, [...(children.get(parent) ?? []), n])
    }
    for (const siblings of children.values())
      siblings.sort(
        (a, b) =>
          a.sortKey.localeCompare(b.sortKey) || a.id.localeCompare(b.id),
      )
    const result: {
      node: Node
      level: number
      hasChildren: boolean
      pos: number
      size: number
    }[] = []
    const visit = (parent: string | null, level: number) => {
      const list = children.get(parent) ?? []
      for (const [i, node] of list.entries()) {
        result.push({
          node,
          level,
          hasChildren: children.has(node.id),
          pos: i + 1,
          size: list.length,
        })
        if (expanded.has(node.id)) visit(node.id, level + 1)
      }
    }
    visit(null, 1)
    return result
  }, [nodes, expanded])
  useEffect(() => {
    if (!selected || scrolledSelection.current === selected) return
    const index = rows.findIndex((row) => row.node.id === selected)
    if (index < 0) return
    scrolledSelection.current = selected
    setStart(index)
    if (container.current) container.current.scrollTop = index * rowHeight
  }, [selected, rows])
  const activeIndex = Math.max(
    0,
    rows.findIndex((row) => row.node.id === active),
  )
  const visibleStart = Math.max(
    0,
    Math.min(start, Math.max(0, rows.length - visibleCount)) - overscan,
  )
  const indices = new Set(
    Array.from(
      {
        length: Math.min(
          rows.length - visibleStart,
          visibleCount + overscan * 2,
        ),
      },
      (_, i) => visibleStart + i,
    ),
  )
  if (rows.length) indices.add(activeIndex)
  const focusRow = (index: number) => {
    const bounded = Math.max(0, Math.min(index, rows.length - 1))
    setActive(rows[bounded]?.node.id)
    if (bounded < start || bounded >= start + visibleCount) {
      setStart(bounded)
      if (container.current) container.current.scrollTop = bounded * rowHeight
    }
  }
  const toggle = (id: string, open: boolean) =>
    setExpanded((previous) => {
      const next = new Set(previous)
      if (open) next.add(id)
      else next.delete(id)
      return next
    })
  return (
    <div
      ref={container}
      role="tree"
      aria-label={text.workspace.tree}
      tabIndex={0}
      aria-activedescendant={
        rows[activeIndex] ? `tree-${rows[activeIndex].node.id}` : undefined
      }
      style={{
        height: visibleCount * rowHeight,
        overflow: 'auto',
        position: 'relative',
      }}
      onScroll={(e) =>
        setStart(Math.floor(e.currentTarget.scrollTop / rowHeight))
      }
      onKeyDown={(e) => {
        const row = rows[activeIndex]
        if (!row) return
        if (
          [
            'ArrowDown',
            'ArrowUp',
            'Home',
            'End',
            'ArrowRight',
            'ArrowLeft',
            'Enter',
            ' ',
          ].includes(e.key)
        )
          e.preventDefault()
        if (e.key === 'ArrowDown') focusRow(activeIndex + 1)
        if (e.key === 'ArrowUp') focusRow(activeIndex - 1)
        if (e.key === 'Home') focusRow(0)
        if (e.key === 'End') focusRow(rows.length - 1)
        if (e.key === 'ArrowRight') {
          if (!expanded.has(row.node.id)) toggle(row.node.id, true)
          else focusRow(activeIndex + 1)
        }
        if (e.key === 'ArrowLeft') {
          if (expanded.has(row.node.id)) toggle(row.node.id, false)
          else focusRow(rows.findIndex((r) => r.node.id === row.node.parentId))
        }
        if (e.key === 'Enter') onSelect(row.node.id)
        if (e.key === ' ') toggle(row.node.id, !expanded.has(row.node.id))
      }}
    >
      <div role="none" style={{ height: rows.length * rowHeight }}>
        {[...indices]
          .sort((a, b) => a - b)
          .map((index) => {
            const { node, level, hasChildren, pos, size } = rows[index]
            return (
              <div
                id={`tree-${node.id}`}
                key={node.id}
                role="treeitem"
                aria-label={node.title}
                aria-level={level}
                aria-posinset={pos}
                aria-setsize={size}
                aria-selected={selected === node.id}
                aria-expanded={hasChildren ? expanded.has(node.id) : undefined}
                className="tree-row"
                style={{
                  position: 'absolute',
                  top: index * rowHeight,
                  height: rowHeight,
                  left: 0,
                  right: 0,
                  paddingLeft: (level - 1) * 14,
                }}
                onClick={() => {
                  setActive(node.id)
                  container.current?.focus()
                  onSelect(node.id)
                }}
              >
                {hasChildren && (
                  <button
                    tabIndex={-1}
                    aria-label={expanded.has(node.id) ? '−' : '+'}
                    onClick={(e) => {
                      e.stopPropagation()
                      toggle(node.id, !expanded.has(node.id))
                    }}
                  >
                    {expanded.has(node.id) ? '▾' : '▸'}
                  </button>
                )}
                <span>{node.title}</span>
              </div>
            )
          })}
      </div>
    </div>
  )
}
