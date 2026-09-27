import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { useState } from 'react'
import { useNavigate } from 'react-router-dom'
import { apiClient, unwrap } from '../api/client'
import type { components } from '../api/generated'
import { ErrorNotice, ModalDialog } from '../admin/shared'
import { useI18n } from '../i18n'
import { pageKey, pageQuery, treeKey, treeQuery } from './queries'
type Page = components['schemas']['Page']
type Node = components['schemas']['PageTreeItem']
type Operation = 'create' | 'child' | 'rename' | 'move' | 'up' | 'archive'

export function PageToolbar({
  spaceId,
  page,
  nodes,
  spaces,
  canCreate,
}: {
  spaceId: string
  page?: Page
  nodes: readonly Node[]
  spaces: readonly components['schemas']['Space'][]
  canCreate: boolean
}) {
  const { messages: text } = useI18n()
  const cache = useQueryClient(),
    navigate = useNavigate()
  const [action, setAction] = useState<Operation | null>(null),
    [title, setTitle] = useState(''),
    [destination, setDestination] = useState(spaceId),
    [parent, setParent] = useState('root')
  const [restricted, setRestricted] = useState(false)
  const canRestrict =
    spaces.find((space) => space.id === spaceId)?.effectiveRole === 'admin'
  const siblings = nodes
    .filter((node) => node.parentId === page?.parentId)
    .sort(
      (a, b) => a.sortKey.localeCompare(b.sortKey) || a.id.localeCompare(b.id),
    )
  const previousSibling =
    siblings[siblings.findIndex((node) => node.id === page?.id) - 1]
  const target = useQuery({
    ...treeQuery(destination),
    enabled: action === 'move' && destination !== spaceId,
  })
  const unresolvedTarget =
    action === 'move' &&
    destination !== spaceId &&
    (target.isFetching || target.isError || target.data === undefined)
  const descendants = new Set<string>(page ? [page.id] : [])
  let changed = true
  while (changed) {
    changed = false
    for (const node of nodes)
      if (
        node.parentId &&
        descendants.has(node.parentId) &&
        !descendants.has(node.id)
      ) {
        descendants.add(node.id)
        changed = true
      }
  }
  const mutation = useMutation({
    mutationFn: async (input: {
      action: Operation
      title: string
      destination: string
      parent: string
      beforeId?: string
      temporaryId: string
      restricted: boolean
    }) => {
      if (input.action === 'create' || input.action === 'child')
        return unwrap(
          await apiClient.POST('/api/v1/pages', {
            body: {
              spaceId,
              parentId: input.action === 'child' ? page?.id : null,
              title: input.title,
              markdown: '',
              properties: {},
              restricted: input.restricted,
            },
          }),
        )
      if (!page) throw new Error('Page required')
      if (input.action === 'rename') {
        const current = await cache.fetchQuery({
          ...pageQuery(page.id),
          staleTime: 0,
        })
        return unwrap(
          await apiClient.PATCH('/api/v1/pages/{id}', {
            params: { path: { id: page.id } },
            body: {
              title: input.title,
              markdown: current.markdown,
              properties: current.properties,
              restricted: current.restricted,
              expectedVersion: current.version,
            },
          }),
        )
      }
      if (input.action === 'archive') {
        unwrap(
          await apiClient.DELETE('/api/v1/pages/{id}', {
            params: { path: { id: page.id } },
          }),
        )
        return null
      }
      unwrap(
        await apiClient.POST('/api/v1/pages/{id}/move', {
          params: { path: { id: page.id } },
          body: {
            spaceId: input.destination,
            parentId: input.parent === 'root' ? null : input.parent,
            beforeId: input.beforeId,
          },
        }),
      )
      return null
    },
    onMutate: async (input) => {
      const keys = [treeKey(spaceId)]
      if (input.action === 'move' && input.destination !== spaceId)
        keys.push(treeKey(input.destination))
      await Promise.all(
        keys.map((queryKey) => cache.cancelQueries({ queryKey, exact: true })),
      )
      const snapshots = keys.map((key) => ({
        key,
        state: cache.getQueryState(key),
      }))
      const old = cache.getQueryData<readonly Node[]>(treeKey(spaceId)) ?? nodes
      let next = [...old]
      if (input.action === 'create' || input.action === 'child')
        next.push({
          id: input.temporaryId,
          parentId: input.action === 'child' ? (page?.id ?? null) : null,
          title: input.title,
          sortKey: 'zzzz',
          version: 1,
          restricted: input.restricted,
          capabilities: { edit: true, managePermissions: canRestrict },
        })
      if (input.action === 'rename')
        next = next.map((n) =>
          n.id === page?.id ? { ...n, title: input.title } : n,
        )
      if (input.action === 'archive')
        next = next.filter((n) => !descendants.has(n.id))
      if ((input.action === 'move' || input.action === 'up') && page) {
        const moved = next
          .filter((n) => descendants.has(n.id))
          .map((n) =>
            n.id === page.id
              ? {
                  ...n,
                  parentId: input.parent === 'root' ? null : input.parent,
                }
              : n,
          )
        if (input.destination !== spaceId) {
          next = next.filter((n) => !descendants.has(n.id))
          cache.setQueryData(treeKey(input.destination), [
            ...(cache.getQueryData<readonly Node[]>(
              treeKey(input.destination),
            ) ?? []),
            ...moved,
          ])
        } else {
          next = next.map((n) => moved.find((m) => m.id === n.id) ?? n)
          if (input.beforeId) {
            const before = next.find((n) => n.id === input.beforeId),
              current = next.find((n) => n.id === page.id)
            if (before && current)
              next = next.map((n) =>
                n.id === page.id
                  ? { ...n, sortKey: before.sortKey }
                  : n.id === before.id
                    ? { ...n, sortKey: current.sortKey }
                    : n,
              )
          }
        }
      }
      cache.setQueryData(treeKey(spaceId), next)
      return { snapshots }
    },
    onError: (_error, _input, context) => {
      for (const { key, state } of context?.snapshots ?? []) {
        if (state)
          cache
            .getQueryCache()
            .find({ queryKey: key, exact: true })
            ?.setState(state)
        else cache.removeQueries({ queryKey: key, exact: true })
      }
    },
    onSuccess: async (result, input) => {
      setAction(null)
      if ((input.action === 'create' || input.action === 'child') && result)
        navigate(`/app/spaces/${spaceId}/pages/${result.id}`)
      if (input.action === 'archive') navigate(`/app/spaces/${spaceId}`)
      if (input.action === 'move' && page)
        navigate(`/app/spaces/${input.destination}/pages/${page.id}`)
      await cache.invalidateQueries({ queryKey: ['workspace', 'tree'] })
      if (page) await cache.invalidateQueries({ queryKey: pageKey(page.id) })
    },
  })
  const open = (operation: Operation) => {
    mutation.reset()
    setRestricted(false)
    setTitle(operation === 'rename' ? (page?.title ?? '') : '')
    setDestination(spaceId)
    setParent(page?.parentId ?? 'root')
    setAction(operation)
  }
  return (
    <div className="page-toolbar">
      <div className="row-actions">
        {canCreate && (
          <button onClick={() => open('create')}>
            {text.workspace.create}
          </button>
        )}
        {page?.capabilities.edit &&
          (['child', 'rename', 'move', 'up', 'archive'] as const).map(
            (operation) => (
              <button
                disabled={
                  mutation.isPending || (operation === 'up' && !previousSibling)
                }
                key={operation}
                onClick={() => open(operation)}
              >
                {text.workspace[operation]}
              </button>
            ),
          )}
      </div>
      {mutation.isError && <ErrorNotice error={mutation.error} />}
      {action && (
        <ModalDialog
          title={text.workspace[action]}
          onCancel={() => {
            if (!mutation.isPending) setAction(null)
          }}
        >
          <form
            onSubmit={(event) => {
              event.preventDefault()
              if (mutation.isPending || unresolvedTarget) return
              if (action === 'up' && !previousSibling) return
              mutation.mutate({
                action,
                title,
                destination,
                parent,
                beforeId: action === 'up' ? previousSibling?.id : undefined,
                temporaryId: `pending-${crypto.randomUUID()}`,
                restricted: canRestrict && restricted,
              })
            }}
          >
            {(action === 'rename' ||
              action === 'create' ||
              action === 'child') && (
              <label>
                {text.workspace.newTitle}
                <input
                  required
                  maxLength={500}
                  value={title}
                  onChange={(e) => setTitle(e.target.value)}
                />
              </label>
            )}
            {(action === 'create' || action === 'child') && canRestrict && (
              <label>
                <input
                  type="checkbox"
                  checked={restricted}
                  onChange={(e) => setRestricted(e.target.checked)}
                />
                {text.workspace.restricted}
              </label>
            )}
            {action === 'move' && (
              <>
                <label>
                  {text.workspace.destination}
                  <select
                    value={destination}
                    onChange={(e) => {
                      setDestination(e.target.value)
                      setParent('root')
                    }}
                  >
                    {spaces
                      .filter((s) =>
                        ['admin', 'owner', 'editor'].includes(s.effectiveRole),
                      )
                      .map((s) => (
                        <option key={s.id} value={s.id}>
                          {s.name}
                        </option>
                      ))}
                  </select>
                </label>
                <label>
                  {text.workspace.parent}
                  <select
                    value={parent}
                    onChange={(e) => setParent(e.target.value)}
                  >
                    <option value="root">{text.workspace.root}</option>
                    {(destination === spaceId ? nodes : (target.data ?? []))
                      .filter(
                        (n) => !descendants.has(n.id) && n.capabilities.edit,
                      )
                      .map((n) => (
                        <option key={n.id} value={n.id}>
                          {n.title}
                        </option>
                      ))}
                  </select>
                </label>
                {target.isError && <ErrorNotice error={target.error} />}
              </>
            )}
            <button
              disabled={mutation.isPending || unresolvedTarget}
              type="submit"
            >
              {text.workspace.apply}
            </button>
            <button
              type="button"
              disabled={mutation.isPending}
              onClick={() => setAction(null)}
            >
              {text.common.cancel}
            </button>
          </form>
        </ModalDialog>
      )}
    </div>
  )
}
