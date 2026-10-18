export type Command = {
  id: string
  label: string
  detail?: string
  defaultShortcut?: string
  enabled?: boolean
  run: () => void | Promise<unknown>
}

export type HotkeyOverrides = Record<string, string | null>
export type KeyInput = Pick<KeyboardEvent, 'key' | 'code' | 'ctrlKey' | 'metaKey' | 'altKey' | 'shiftKey' | 'isComposing' | 'repeat'> & { getModifierState?: (key: string) => boolean }

// Stored bindings use physical keys, independent of the current RU/EN layout.
export function shortcutFromEvent(event: KeyInput): string | null {
  if (event.isComposing || event.getModifierState?.('AltGraph')) return null
  if (/^(Control|Meta|Alt|Shift)(Left|Right)$/.test(event.code)) return null
  const code = event.code || (event.key === '+' ? 'Equal' : event.key)
  return [...(event.ctrlKey || event.metaKey ? ['Mod'] : []), ...(event.altKey ? ['Alt'] : []), ...(event.shiftKey ? ['Shift'] : []), code].join('+')
}

export function validShortcut(value: string): boolean {
  return /^(?:(?:Mod\+)(?:Alt\+)?(?:Shift\+)?|Alt\+(?:Shift\+)?)(?:Key[A-Z]|Digit[0-9]|Arrow(?:Left|Right|Up|Down)|Tab|Space|Enter|Comma|Period|Slash|Backslash|Semicolon|Quote|BracketLeft|BracketRight|Minus|Equal|Backquote|F(?:[1-9]|1[0-2]))$/.test(value)
}

const reserved = new Set(['Mod+KeyA', 'Mod+KeyC', 'Mod+KeyX', 'Mod+KeyV', 'Mod+KeyZ', 'Mod+Shift+KeyZ', 'Mod+KeyY', 'Mod+KeyF', 'Mod+KeyR', 'Alt+F4'])
export function effectiveShortcut(command: Command, overrides: HotkeyOverrides): string | null {
  return Object.hasOwn(overrides, command.id) ? overrides[command.id] : command.defaultShortcut ?? null
}

export function shortcutLabel(shortcut: string | null | undefined): string {
  if (!shortcut) return ''
  const labels: Record<string, string> = { Mod: 'Ctrl', Comma: ',', Period: '.', Slash: '/', Equal: '+', Minus: '−', ArrowLeft: '←', ArrowRight: '→', ArrowUp: '↑', ArrowDown: '↓', Space: 'Space' }
  return shortcut.split('+').map((part) => labels[part] ?? part.replace(/^(Key|Digit)/, '')).join(' + ')
}

export function bindingError(id: string, value: string | null, commands: Command[], overrides: HotkeyOverrides): string | null {
  if (!commands.some((command) => command.id === id)) return 'Неизвестная команда'
  if (value === null) return null
  if (!validShortcut(value)) return 'Используйте Ctrl или Alt и клавишу; Shift можно добавить.'
  if (reserved.has(value) && commands.find((command) => command.id === id)?.defaultShortcut !== value) return 'Это сочетание зарезервировано для ввода, буфера обмена или окна.'
  const collision = commands.find((command) => command.id !== id && effectiveShortcut(command, overrides) === value)
  return collision ? `Уже назначено: ${collision.label}. Сначала снимите это назначение.` : null
}

export function createCommandRegistry(commands: Command[], overrides: HotkeyOverrides = {}) {
  const byId = new Map<string, Command>()
  for (const command of commands) {
    if (byId.has(command.id)) throw new Error(`Duplicate command: ${command.id}`)
    byId.set(command.id, command)
  }
  return {
    commands,
    get: (id: string) => byId.get(id),
    shortcut: (id: string) => { const command = byId.get(id); return command ? shortcutLabel(effectiveShortcut(command, overrides)) : '' },
    execute(id: string) {
      const command = byId.get(id)
      if (!command || command.enabled === false) return false
      return command.run() ?? true
    },
    forEvent(event: KeyInput) {
      const shortcut = shortcutFromEvent(event)
      if (!shortcut) return undefined
      // Corrupt/imported duplicate bindings fail closed instead of picking one.
      const bindingMatches = (value: string) => commands.filter((command) => {
        const binding = effectiveShortcut(command, overrides)
        return binding === value && validShortcut(binding) && (!reserved.has(binding) || command.defaultShortcut === binding)
      })
      let matches = bindingMatches(shortcut)
      if (!matches.length && shortcut === 'Mod+Shift+Equal') matches = bindingMatches('Mod+Equal')
      return matches.length === 1 && matches[0].enabled !== false ? matches[0] : undefined
    },
  }
}

export type CommandRegistry = ReturnType<typeof createCommandRegistry>
