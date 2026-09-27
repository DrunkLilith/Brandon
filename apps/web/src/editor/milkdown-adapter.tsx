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
import {
  useEffect,
  useImperativeHandle,
  useLayoutEffect,
  useRef,
  useState,
  type Ref,
} from 'react'
import { ErrorNotice } from '../admin/shared'
import { useI18n } from '../i18n'

export type MarkdownHandle = { flush(): string }

export function MilkdownAdapter({
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
  const root = useRef<HTMLDivElement>(null),
    editor = useRef<Editor | null>(null)
  const visualMarkdownLabel = useRef(text.workspace.visualMarkdown)
  const latest = useRef(value),
    change = useRef(onChange),
    locked = useRef(readOnly),
    emitted = useRef(value)
  const replacing = useRef(false)
  const pending = useRef(false)
  const timer = useRef<ReturnType<typeof setTimeout> | null>(null)
  const dirty = useRef(onDirty),
    unmount = useRef(onUnmount)
  const [error, setError] = useState<unknown>()
  latest.current = value
  change.current = onChange
  locked.current = readOnly
  dirty.current = onDirty
  unmount.current = onUnmount
  visualMarkdownLabel.current = text.workspace.visualMarkdown
  const cancelTimer = () => {
    if (timer.current !== null) clearTimeout(timer.current)
    timer.current = null
  }
  const flush = (notify = true) => {
    cancelTimer()
    if (pending.current && editor.current) {
      const markdown = editor.current.action((ctx) =>
        ctx.get(serializerCtx)(ctx.get(editorViewCtx).state.doc),
      )
      pending.current = false
      if (markdown !== emitted.current) {
        emitted.current = markdown
        if (notify) change.current(markdown)
        else unmount.current?.(markdown)
      }
    }
    return emitted.current
  }
  const flushCurrent = useRef(flush)
  flushCurrent.current = flush
  useImperativeHandle(ref, () => ({ flush: () => flushCurrent.current() }), [])
  const replaceValue = (instance: Editor, markdown: string) => {
    cancelTimer()
    pending.current = false
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
    const persist = () => flushCurrent.current()
    const onVisibility = () => { if (document.visibilityState === 'hidden') persist() }
    window.addEventListener('pagehide', persist)
    window.addEventListener('beforeunload', persist)
    window.addEventListener('knowledge:flush-drafts', persist)
    document.addEventListener('visibilitychange', onVisibility)
    const instance = Editor.make()
      .config((ctx) => {
        ctx.set(rootCtx, root.current)
        ctx.set(defaultValueCtx, latest.current)
        ctx.update(editorViewOptionsCtx, (options) => ({
          ...options,
          editable: () => !locked.current,
          // Keep the authoritative document current cheaply. Full Markdown
          // serialization/persistence is coalesced, with explicit boundary flushes.
          dispatchTransaction(transaction) {
            const view = ctx.get(editorViewCtx)
            const previous = view.state.doc
            view.updateState(view.state.apply(transaction))
            if (
              !disposed &&
              !replacing.current &&
              !previous.eq(view.state.doc)
            ) {
              if (!pending.current) dirty.current?.()
              pending.current = true
              cancelTimer()
              timer.current = setTimeout(() => flushCurrent.current(), 200)
            }
          },
          attributes: {
            role: 'textbox',
            'aria-label': visualMarkdownLabel.current,
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
        instance.action((ctx) => {
          ctx.get(editorViewCtx).dom.setAttribute('aria-label', visualMarkdownLabel.current)
        })
      })
      .catch((reason) => {
        if (!disposed) setError(reason)
      })
    return () => {
      window.removeEventListener('pagehide', persist)
      window.removeEventListener('beforeunload', persist)
      window.removeEventListener('knowledge:flush-drafts', persist)
      document.removeEventListener('visibilitychange', onVisibility)
      // Persist the last edit without updating an unmounting React component.
      flushCurrent.current(false)
      disposed = true
      editor.current = null
      void instance.destroy()
    }
  }, [])
  useLayoutEffect(() => {
    // ProseMirror caches editable and its DOM attribute on view updates.
    // Refresh the same state before paint; do not dispatch a document edit.
    editor.current?.action((ctx) => {
      const view = ctx.get(editorViewCtx)
      view.updateState(view.state)
    })
  }, [readOnly])
  useLayoutEffect(() => {
    editor.current?.action((ctx) => {
      ctx.get(editorViewCtx).dom.setAttribute('aria-label', text.workspace.visualMarkdown)
    })
  }, [text.workspace.visualMarkdown])
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
