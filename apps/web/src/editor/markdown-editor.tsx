import { useState } from 'react'
import { MilkdownAdapter } from './milkdown-adapter'
import { workspaceMessages as text } from '../pages/messages'
export function MarkdownEditor({
  value,
  onChange,
  readOnly = false,
}: {
  value: string
  onChange(value: string): void
  readOnly?: boolean
}) {
  const [mode, setMode] = useState<'rich' | 'raw' | 'preview'>('rich')
  return (
    <section>
      <div className="row-actions">
        {(['rich', 'raw', 'preview'] as const).map((m) => (
          <button
            key={m}
            type="button"
            aria-pressed={mode === m}
            onClick={() => setMode(m)}
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
          key={mode}
          value={value}
          onChange={onChange}
          readOnly={readOnly || mode === 'preview'}
        />
      )}
    </section>
  )
}
