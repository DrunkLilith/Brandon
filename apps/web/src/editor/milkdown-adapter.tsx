import {
  Editor,
  rootCtx,
  defaultValueCtx,
  editorViewOptionsCtx,
} from '@milkdown/core'
import { commonmark, imageSchema } from '@milkdown/preset-commonmark'
import { listener, listenerCtx } from '@milkdown/plugin-listener'
import { replaceAll } from '@milkdown/utils'
import { useEffect, useRef, useState } from 'react'
import { ErrorNotice } from '../admin/shared'

export function MilkdownAdapter({
  value,
  onChange,
  readOnly = false,
}: {
  value: string
  onChange(value: string): void
  readOnly?: boolean
}) {
  const root = useRef<HTMLDivElement>(null),
    editor = useRef<Editor | null>(null)
  const latest = useRef(value),
    change = useRef(onChange),
    locked = useRef(readOnly),
    emitted = useRef(value)
  const [error, setError] = useState<unknown>()
  latest.current = value
  change.current = onChange
  locked.current = readOnly
  useEffect(() => {
    let disposed = false
    const instance = Editor.make()
      .config((ctx) => {
        ctx.set(rootCtx, root.current)
        ctx.set(defaultValueCtx, latest.current)
        ctx.update(editorViewOptionsCtx, (options) => ({
          ...options,
          editable: () => !locked.current,
          attributes: {
            role: 'textbox',
            'aria-label': 'Визуальный Markdown',
            'aria-multiline': 'true',
          },
        }))
        // CommonMark HTML nodes are literal text. Image nodes stay textual too: no remote/data image requests.
        ctx.update(imageSchema.key, (original) => (ctx) => ({
          ...original(ctx),
          toDOM: (node) => [
            'span',
            { 'data-image-src': node.attrs.src },
            node.attrs.alt || node.attrs.src,
          ],
        }))
        ctx.get(listenerCtx).markdownUpdated((_ctx, markdown, previous) => {
          if (markdown !== previous && !disposed) {
            emitted.current = markdown
            change.current(markdown)
          }
        })
      })
      .use(commonmark)
      .use(listener)
    void instance
      .create()
      .then(() => {
        if (disposed) {
          void instance.destroy()
          return
        }
        editor.current = instance
        if (latest.current !== value)
          instance.action(replaceAll(latest.current))
      })
      .catch((reason) => {
        if (!disposed) setError(reason)
      })
    return () => {
      disposed = true
      editor.current = null
      void instance.destroy()
    }
  }, [])
  useEffect(() => {
    if (editor.current && value !== emitted.current) {
      emitted.current = value
      editor.current.action(replaceAll(value))
    }
  }, [value])
  return (
    <>
      <div className="markdown-rich" ref={root} />
      {error && <ErrorNotice error={error} />}
    </>
  )
}
