import {
  Editor,
  rootCtx,
  defaultValueCtx,
  editorViewOptionsCtx,
  editorViewCtx,
  serializerCtx,
} from '@milkdown/core'
import { commonmark, imageSchema } from '@milkdown/preset-commonmark'
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
  const replacing = useRef(false)
  const [error, setError] = useState<unknown>()
  latest.current = value
  change.current = onChange
  locked.current = readOnly
  const replaceValue = (instance: Editor, markdown: string) => {
    emitted.current = markdown
    replacing.current = true
    try {
      instance.action(replaceAll(markdown))
    } finally {
      replacing.current = false
    }
  }
  useEffect(() => {
    let disposed = false
    const instance = Editor.make()
      .config((ctx) => {
        ctx.set(rootCtx, root.current)
        ctx.set(defaultValueCtx, latest.current)
        ctx.update(editorViewOptionsCtx, (options) => ({
          ...options,
          editable: () => !locked.current,
          // The listener plugin debounces document events and cancels them on
          // destroy. Publish the authoritative, fully applied document now so
          // save, mode changes and navigation never see a stale draft.
          dispatchTransaction(transaction) {
            const view = ctx.get(editorViewCtx)
            const previous = view.state.doc
            view.updateState(view.state.apply(transaction))
            if (
              !disposed &&
              !replacing.current &&
              !previous.eq(view.state.doc)
            ) {
              const markdown = ctx.get(serializerCtx)(view.state.doc)
              if (markdown !== emitted.current) {
                emitted.current = markdown
                change.current(markdown)
              }
            }
          },
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
      })
      .use(commonmark)
    void instance
      .create()
      .then(() => {
        if (disposed) {
          void instance.destroy()
          return
        }
        editor.current = instance
        if (latest.current !== value) replaceValue(instance, latest.current)
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
      replaceValue(editor.current, value)
    }
  }, [value])
  return (
    <>
      <div className="markdown-rich" ref={root} />
      {error && <ErrorNotice error={error} />}
    </>
  )
}
