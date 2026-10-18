import { useLayoutEffect, useRef, useState } from 'react'
import type { CommandRegistry } from './commands'

export function CommandMenu({ x, y, ids, registry, disabled = [], onExecute, onClose }: {
  x: number; y: number; ids: (string | null)[]; registry: CommandRegistry; disabled?: string[]
  onExecute: (id: string) => void; onClose: (restoreFocus?: boolean) => void
}) {
  const ref = useRef<HTMLDivElement>(null)
  const close = useRef(onClose)
  close.current = onClose
  const [position, setPosition] = useState({ x, y })
  useLayoutEffect(() => {
    const menu = ref.current
    if (!menu) return
    const rect = menu.getBoundingClientRect()
    setPosition({ x: Math.max(8, Math.min(x, window.innerWidth - rect.width - 8)), y: Math.max(48, Math.min(y, window.innerHeight - rect.height - 8)) })
    menu.querySelector<HTMLButtonElement>('button:not(:disabled)')?.focus()
    const dismiss = (event: PointerEvent) => { if (!menu.contains(event.target as Node)) close.current(false) }
    const scroll = () => close.current(false)
    document.addEventListener('pointerdown', dismiss, true)
    window.addEventListener('resize', scroll)
    return () => { document.removeEventListener('pointerdown', dismiss, true); window.removeEventListener('resize', scroll) }
  }, [x, y])
  return <div ref={ref} className="command-menu" role="menu" aria-label="Действия редактора" style={{ left: position.x, top: position.y }} onContextMenu={(event) => event.preventDefault()} onKeyDown={(event) => {
    event.stopPropagation() // Enter must activate this menu, not the underlying Canvas.
    if (event.key === 'Escape' || event.key === 'Tab') { event.preventDefault(); event.stopPropagation(); onClose(); return }
    if (!['ArrowDown', 'ArrowUp', 'Home', 'End'].includes(event.key)) return
    event.preventDefault()
    const buttons = Array.from(ref.current?.querySelectorAll<HTMLButtonElement>('button:not(:disabled)') ?? [])
    const index = buttons.indexOf(document.activeElement as HTMLButtonElement)
    const next = event.key === 'Home' ? 0 : event.key === 'End' ? buttons.length - 1 : (index + (event.key === 'ArrowUp' ? -1 : 1) + buttons.length) % buttons.length
    buttons[next]?.focus()
  }}>{ids.map((id, position) => {
    if (!id) return <hr key={`separator-${position}`} role="separator" />
    const command = registry.get(id)
    return command && <button key={id} role="menuitem" disabled={command.enabled === false || disabled.includes(id)} onClick={() => onExecute(id)}><span>{command.label}</span>{registry.shortcut(id) && <kbd>{registry.shortcut(id)}</kbd>}</button>
  })}</div>
}
