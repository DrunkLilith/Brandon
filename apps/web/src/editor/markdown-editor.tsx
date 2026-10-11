import { useImperativeHandle, useRef, useState, type Ref } from 'react'
import { MilkdownAdapter, type MarkdownHandle } from './milkdown-adapter'
import { useI18n } from '../i18n'
export function MarkdownEditor({
  ref,
  value,
  onChange,
  onDirty,
  onUnmount,
  readOnly = false,
}: {
  ref?: Ref<MarkdownHandle>
  value: string
  onChange(value: string): void
  onDirty?(): void
  onUnmount?(value: string): void
  readOnly?: boolean
}) {
  const { messages: text } = useI18n()
  const [mode, setMode] = useState<'rich' | 'raw' | 'preview'>('rich')
  const rich = useRef<MarkdownHandle>(null)
  useImperativeHandle(ref, () => ({
    flush: () => rich.current?.flush() ?? value,
  }))
  return (
    <section className="markdown-editor">
      <div className="editor-mode-switch">
        {(['rich', 'raw', 'preview'] as const).map((m) => (
          <button
            key={m}
            type="button"
            aria-pressed={mode === m}
            onClick={() => {
              rich.current?.flush()
              setMode(m)
            }}
          >
            {text.workspace[m]}
          </button>
        ))}
      </div>
      {mode === 'raw' ? (
        <textarea
          className="markdown-raw"
          aria-label="Markdown"
          value={value}
          readOnly={readOnly}
          onChange={(e) => onChange(e.target.value)}
        />
      ) : (
        <MilkdownAdapter
          ref={rich}
          key={mode}
          value={value}
          onChange={onChange}
          onDirty={onDirty}
          onUnmount={onUnmount}
          readOnly={readOnly || mode === 'preview'}
        />
      )}
    </section>
  )
}
