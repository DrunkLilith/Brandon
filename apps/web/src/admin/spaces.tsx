import { useInfiniteQuery, useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { useEffect, useState, type FormEvent } from 'react'

import { apiClient, unwrap, unwrapArray } from '../api/client'
import type { components } from '../api/generated'
import { useI18n } from '../i18n'
import { ErrorNotice, LoadingBlock, PageHeader } from './shared'

type Space = components['schemas']['Space']
type Subject = components['schemas']['Subject']
type SubjectType = Subject['type']
type GrantRole = components['schemas']['SpaceGrant']['role']
type SpaceGrant = components['schemas']['SpaceGrant']
const spacesKey = ['admin', 'spaces'] as const
const archivedSpacesKey = ['admin', 'spaces', 'archived'] as const

function SpaceEditor({ space }: { space: Space }) {
  const { messages: text } = useI18n()
  const queryClient = useQueryClient()
  const [name, setName] = useState(space.name)
  const [description, setDescription] = useState(space.description ?? '')
  useEffect(() => { setName(space.name); setDescription(space.description ?? '') }, [space])
  const update = useMutation({
    mutationFn: async () => unwrap(await apiClient.PATCH('/api/v1/spaces/{id}', { params: { path: { id: space.id } }, body: { name: name.trim(), description: description.trim() || null } })),
    onSuccess: () => queryClient.invalidateQueries({ queryKey: spacesKey }),
  })
  return <details className="resource-editor"><summary>{text.spaces.edit}</summary><div className="inline-form"><label>{text.spaces.selectedName}<input value={name} onChange={(event) => setName(event.target.value)} /></label><label>{text.spaces.selectedDescription}<input value={description} onChange={(event) => setDescription(event.target.value)} /></label><button type="button" onClick={() => update.mutate()} disabled={!name.trim() || update.isPending}>{text.spaces.save}</button></div>{update.isError && <ErrorNotice error={update.error} />}</details>
}

function SpaceAccess({ space }: { space: Space }) {
  const { messages: text } = useI18n()
  const queryClient = useQueryClient()
  const [subjectType, setSubjectType] = useState<SubjectType>('user')
  const [subjectId, setSubjectId] = useState('')
  const [role, setRole] = useState<GrantRole>('viewer')
  const [validation, setValidation] = useState<'subject' | null>(null)
  const grantsKey = ['admin', 'spaces', space.id, 'grants'] as const
  const subjects = useInfiniteQuery({
    queryKey: ['admin', 'spaces', space.id, 'subjects'],
    initialPageParam: undefined as string | undefined,
    queryFn: async ({ pageParam }) => unwrap(await apiClient.GET('/api/v1/spaces/{id}/subjects', { params: { path: { id: space.id }, query: { limit: 100, after: pageParam } } })),
    getNextPageParam: (lastPage) => lastPage.nextCursor ?? undefined,
  })
  const grants = useQuery({ queryKey: grantsKey, queryFn: async () => unwrapArray(await apiClient.GET('/api/v1/spaces/{space_id}/grants', { params: { path: { space_id: space.id } } })) })
  const create = useMutation({
    mutationFn: async () => unwrap(await apiClient.POST('/api/v1/spaces/{space_id}/grants', { params: { path: { space_id: space.id } }, body: { subjectId, subjectType, role } })),
    onSuccess: () => { setSubjectId(''); queryClient.invalidateQueries({ queryKey: grantsKey }) },
  })
  const allSubjects = subjects.data?.pages.flatMap((page) => page.items) ?? []
  const available = allSubjects.filter((subject) => subject.type === subjectType)
  const submit = (event: FormEvent) => {
    event.preventDefault()
    if (!subjectId || !available.some((subject) => subject.id === subjectId)) return setValidation('subject')
    setValidation(null)
    create.mutate()
  }
  return <section className="admin-panel" aria-label={text.spaces.accessFor(space.name)}><h2>{text.spaces.accessTitle(space.name)}</h2>
    <form className="inline-form grant-form" onSubmit={submit}>
      <label>{text.spaces.subjectType}<select value={subjectType} onChange={(event) => { setSubjectType(event.target.value as SubjectType); setSubjectId('') }}><option value="user">{text.labels.subjectType.user}</option><option value="group">{text.labels.subjectType.group}</option></select></label>
      <label>{text.spaces.subject}<select value={subjectId} onChange={(event) => setSubjectId(event.target.value)}><option value="">{text.spaces.select}</option>{available.map((subject) => <option value={subject.id} key={subject.id}>{subject.label}</option>)}</select></label>
      <label>{text.spaces.accessRole}<select value={role} onChange={(event) => setRole(event.target.value as GrantRole)}><option value="viewer">{text.labels.grantRole.viewer}</option><option value="editor">{text.labels.grantRole.editor}</option><option value="owner">{text.labels.grantRole.owner}</option></select></label>
      <button type="submit" disabled={create.isPending}>{text.spaces.addGrant}</button>
    </form>
    {subjects.hasNextPage && <button type="button" onClick={() => subjects.fetchNextPage()} disabled={subjects.isFetchingNextPage}>{text.spaces.loadMoreSubjects}</button>}
    {validation && <p role="alert" className="form-error">{text.spaces.invalidSubject}</p>}
    {subjects.isError || grants.isError || create.isError ? <ErrorNotice error={subjects.error ?? grants.error ?? create.error} /> : grants.isPending ? <LoadingBlock /> : <ul className="admin-list">{grants.data.map((grant) => {
      const subject = allSubjects.find((item) => item.id === grant.subjectId)
      return <GrantRow key={grant.id} spaceId={space.id} grant={grant} subjectLabel={subject?.label ?? text.spaces.unavailableSubject} grantsKey={grantsKey} />
    })}</ul>}
  </section>
}

function GrantRow({ spaceId, grant, subjectLabel, grantsKey }: { spaceId: string; grant: SpaceGrant; subjectLabel: string; grantsKey: readonly string[] }) {
  const { messages: text } = useI18n()
  const queryClient = useQueryClient()
  const [draftRole, setDraftRole] = useState<GrantRole>(grant.role)
  useEffect(() => { setDraftRole(grant.role) }, [grant.role])
  const update = useMutation({
    mutationFn: async (nextRole: GrantRole) => unwrap(await apiClient.PATCH('/api/v1/spaces/{space_id}/grants/{grant_id}', { params: { path: { space_id: spaceId, grant_id: grant.id } }, body: { role: nextRole } })),
    onSuccess: () => queryClient.invalidateQueries({ queryKey: grantsKey }),
  })
  const remove = useMutation({
    mutationFn: async () => unwrap(await apiClient.DELETE('/api/v1/spaces/{space_id}/grants/{grant_id}', { params: { path: { space_id: spaceId, grant_id: grant.id } } })),
    onSuccess: () => queryClient.invalidateQueries({ queryKey: grantsKey }),
  })
  const pending = update.isPending || remove.isPending
  const changeRole = (nextRole: GrantRole) => {
    setDraftRole(nextRole)
    update.mutate(nextRole)
  }
  return <li className="admin-list-item"><span>{subjectLabel} · {text.labels.subjectType[grant.subjectType]}</span><div className="row-actions"><label><span className="sr-only">{text.spaces.roleFor(subjectLabel)}</span><select value={draftRole} disabled={pending} onChange={(event) => changeRole(event.target.value as GrantRole)}><option value="viewer">{text.labels.grantRole.viewer}</option><option value="editor">{text.labels.grantRole.editor}</option><option value="owner">{text.labels.grantRole.owner}</option></select></label><button type="button" onClick={() => remove.mutate()} disabled={pending}>{text.common.remove}</button></div>{update.isError && <><ErrorNotice error={update.error} /><button type="button" onClick={() => update.mutate(draftRole)} disabled={pending}>{text.spaces.retryRole}</button></>}{remove.isError && <ErrorNotice error={remove.error} />}</li>
}

export function SpacesPage() {
  const { messages: text } = useI18n()
  const queryClient = useQueryClient()
  const [selectedId, setSelectedId] = useState('')
  const [name, setName] = useState('')
  const [description, setDescription] = useState('')
  const spaces = useQuery({ queryKey: spacesKey, queryFn: async () => unwrapArray(await apiClient.GET('/api/v1/spaces')) })
  const archived = useInfiniteQuery({
    queryKey: archivedSpacesKey,
    initialPageParam: undefined as string | undefined,
    queryFn: async ({ pageParam }) => unwrap(await apiClient.GET('/api/v1/spaces/archived', { params: { query: { limit: 100, after: pageParam } } })),
    getNextPageParam: (lastPage) => lastPage.nextCursor ?? undefined,
  })
  useEffect(() => {
    if (spaces.data && !spaces.data.some((space) => space.id === selectedId)) setSelectedId(spaces.data[0]?.id ?? '')
  }, [selectedId, spaces.data])
  const create = useMutation({ mutationFn: async () => unwrap(await apiClient.POST('/api/v1/spaces', { body: { name: name.trim(), description: description.trim() || null } })), onSuccess: () => { setName(''); setDescription(''); queryClient.invalidateQueries({ queryKey: spacesKey }) } })
  const archive = useMutation({ mutationFn: async (id: string) => unwrap(await apiClient.DELETE('/api/v1/spaces/{id}', { params: { path: { id } } })), onSuccess: () => { queryClient.invalidateQueries({ queryKey: spacesKey }); queryClient.invalidateQueries({ queryKey: archivedSpacesKey }) } })
  const restore = useMutation({ mutationFn: async (id: string) => unwrap(await apiClient.POST('/api/v1/spaces/{id}/restore', { params: { path: { id } } })), onSuccess: () => { queryClient.invalidateQueries({ queryKey: spacesKey }); queryClient.invalidateQueries({ queryKey: archivedSpacesKey }) } })
  const selected = spaces.data?.find((space) => space.id === selectedId)
  return <section><PageHeader title={text.pages.spaces} />
    <section className="admin-panel"><h2>{text.spaces.create}</h2><form className="inline-form" onSubmit={(event) => { event.preventDefault(); if (name.trim()) create.mutate() }}><label>{text.spaces.name}<input value={name} onChange={(event) => setName(event.target.value)} /></label><label>{text.common.description}<input value={description} onChange={(event) => setDescription(event.target.value)} /></label><button type="submit" disabled={!name.trim() || create.isPending}>{text.common.create}</button></form>{create.isError && <ErrorNotice error={create.error} />}</section>
    <section className="admin-panel"><h2>{text.spaces.active}</h2>{spaces.isPending ? <LoadingBlock /> : spaces.isError ? <ErrorNotice error={spaces.error} /> : <><div className="resource-selector"><label>{text.spaces.selector}<select value={selectedId} onChange={(event) => setSelectedId(event.target.value)}>{spaces.data.map((space) => <option value={space.id} key={space.id}>{space.name}</option>)}</select></label>{selected && <button type="button" onClick={() => archive.mutate(selected.id)} disabled={archive.isPending}>{text.common.archive}</button>}</div>{selected && <SpaceEditor space={selected} />}</>}{archive.isError && <ErrorNotice error={archive.error} />}</section>
    {selected && <SpaceAccess space={selected} />}
    <section className="admin-panel"><h2>{text.spaces.archive}</h2>{archived.isPending ? <LoadingBlock /> : archived.isError ? <ErrorNotice error={archived.error} /> : archived.data.pages.some((page) => page.items.length) ? <><ul className="admin-list">{archived.data.pages.flatMap((page) => page.items).map((item) => <li className="admin-list-item" key={item.id}><span>{item.name}</span><button type="button" onClick={() => restore.mutate(item.id)} disabled={restore.isPending}>{text.common.restore}</button></li>)}</ul>{archived.hasNextPage && <button type="button" onClick={() => archived.fetchNextPage()} disabled={archived.isFetchingNextPage}>{text.spaces.loadMoreArchived}</button>}</> : <p className="notice">{text.common.emptyArchive}</p>}{restore.isError && <ErrorNotice error={restore.error} />}</section>
  </section>
}
