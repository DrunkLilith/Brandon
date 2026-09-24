import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { useEffect, useState, type FormEvent } from 'react'

import { apiClient, unwrap } from '../api/client'
import type { components } from '../api/generated'
import { adminMessages as text } from './messages'
import { apiArray, apiObject, ErrorNotice, LoadingBlock, PageHeader } from './shared'

type Space = components['schemas']['Space']
type Subject = components['schemas']['Subject']
type SubjectType = Subject['type']
type GrantRole = components['schemas']['SpaceGrant']['role']
type SpaceGrant = components['schemas']['SpaceGrant']
type SubjectList = components['schemas']['SubjectList']
type ArchivedList = components['schemas']['ArchivedList']

const spacesKey = ['admin', 'spaces'] as const
const archivedSpacesKey = ['admin', 'spaces', 'archived'] as const

function SpaceEditor({ space }: { space: Space }) {
  const queryClient = useQueryClient()
  const [name, setName] = useState(space.name)
  const [description, setDescription] = useState(space.description ?? '')
  useEffect(() => { setName(space.name); setDescription(space.description ?? '') }, [space])
  const update = useMutation({
    mutationFn: async () => unwrap(await apiClient.PATCH('/api/v1/spaces/{id}', { params: { path: { id: space.id } }, body: { name: name.trim(), description: description.trim() || null } })),
    onSuccess: () => queryClient.invalidateQueries({ queryKey: spacesKey }),
  })
  return <details className="resource-editor"><summary>Изменить пространство</summary><div className="inline-form"><label>Название выбранного пространства<input value={name} onChange={(event) => setName(event.target.value)} /></label><label>Описание выбранного пространства<input value={description} onChange={(event) => setDescription(event.target.value)} /></label><button type="button" onClick={() => update.mutate()} disabled={!name.trim() || update.isPending}>Сохранить пространство</button></div>{update.isError && <ErrorNotice error={update.error} />}</details>
}

function SpaceAccess({ space }: { space: Space }) {
  const queryClient = useQueryClient()
  const [subjectType, setSubjectType] = useState<SubjectType>('user')
  const [subjectId, setSubjectId] = useState('')
  const [role, setRole] = useState<GrantRole>('viewer')
  const [validation, setValidation] = useState('')
  const grantsKey = ['admin', 'spaces', space.id, 'grants'] as const
  const subjects = useQuery({ queryKey: ['admin', 'spaces', space.id, 'subjects'], queryFn: async () => apiObject<SubjectList>(unwrap(await apiClient.GET('/api/v1/spaces/{id}/subjects', { params: { path: { id: space.id }, query: { limit: 100 } } }))) })
  const grants = useQuery({ queryKey: grantsKey, queryFn: async () => apiArray<SpaceGrant>(unwrap(await apiClient.GET('/api/v1/spaces/{space_id}/grants', { params: { path: { space_id: space.id } } }))) })
  const create = useMutation({
    mutationFn: async () => unwrap(await apiClient.POST('/api/v1/spaces/{space_id}/grants', { params: { path: { space_id: space.id } }, body: { subjectId, subjectType, role } })),
    onSuccess: () => { setSubjectId(''); queryClient.invalidateQueries({ queryKey: grantsKey }) },
  })
  const update = useMutation({
    mutationFn: async ({ grantId, nextRole }: { grantId: string; nextRole: GrantRole }) => unwrap(await apiClient.PATCH('/api/v1/spaces/{space_id}/grants/{grant_id}', { params: { path: { space_id: space.id, grant_id: grantId } }, body: { role: nextRole } })),
    onSuccess: () => queryClient.invalidateQueries({ queryKey: grantsKey }),
  })
  const remove = useMutation({
    mutationFn: async (grantId: string) => unwrap(await apiClient.DELETE('/api/v1/spaces/{space_id}/grants/{grant_id}', { params: { path: { space_id: space.id, grant_id: grantId } } })),
    onSuccess: () => queryClient.invalidateQueries({ queryKey: grantsKey }),
  })
  const available = subjects.data?.items.filter((subject) => subject.type === subjectType) ?? []
  const submit = (event: FormEvent) => {
    event.preventDefault()
    if (!subjectId || !available.some((subject) => subject.id === subjectId)) return setValidation('Выберите пользователя или группу из доступного списка.')
    setValidation('')
    create.mutate()
  }
  return <section className="admin-panel" aria-label={`Права доступа ${space.name}`}><h2>Права доступа: {space.name}</h2>
    <form className="inline-form grant-form" onSubmit={submit}>
      <label>Тип субъекта<select value={subjectType} onChange={(event) => { setSubjectType(event.target.value as SubjectType); setSubjectId('') }}><option value="user">Пользователь</option><option value="group">Группа</option></select></label>
      <label>Пользователь или группа<select value={subjectId} onChange={(event) => setSubjectId(event.target.value)}><option value="">Выберите</option>{available.map((subject) => <option value={subject.id} key={subject.id}>{subject.label}</option>)}</select></label>
      <label>Роль доступа<select value={role} onChange={(event) => setRole(event.target.value as GrantRole)}><option value="viewer">Читатель</option><option value="editor">Редактор</option><option value="owner">Владелец</option></select></label>
      <button type="submit" disabled={create.isPending}>Добавить право доступа</button>
    </form>
    {validation && <p role="alert" className="form-error">{validation}</p>}
    {subjects.isError || grants.isError || create.isError ? <ErrorNotice error={subjects.error ?? grants.error ?? create.error} /> : grants.isPending ? <LoadingBlock /> : <ul className="admin-list">{grants.data.map((grant) => {
      const subject = subjects.data?.items.find((item) => item.id === grant.subjectId)
      return <li className="admin-list-item" key={grant.id}><span>{subject?.label ?? 'Недоступный субъект'} · {grant.subjectType}</span><div className="row-actions"><label><span className="sr-only">Роль для {subject?.label ?? grant.subjectId}</span><select value={grant.role} onChange={(event) => update.mutate({ grantId: grant.id, nextRole: event.target.value as GrantRole })}><option value="viewer">Читатель</option><option value="editor">Редактор</option><option value="owner">Владелец</option></select></label><button type="button" onClick={() => remove.mutate(grant.id)}>Удалить</button></div></li>
    })}</ul>}
  </section>
}

export function SpacesPage() {
  const queryClient = useQueryClient()
  const [selectedId, setSelectedId] = useState('')
  const [name, setName] = useState('')
  const [description, setDescription] = useState('')
  const spaces = useQuery({ queryKey: spacesKey, queryFn: async () => apiArray<Space>(unwrap(await apiClient.GET('/api/v1/spaces'))) })
  const archived = useQuery({ queryKey: archivedSpacesKey, queryFn: async () => apiObject<ArchivedList>(unwrap(await apiClient.GET('/api/v1/spaces/archived', { params: { query: { limit: 100 } } }))) })
  useEffect(() => { if (!selectedId && spaces.data?.[0]) setSelectedId(spaces.data[0].id) }, [selectedId, spaces.data])
  const create = useMutation({ mutationFn: async () => unwrap(await apiClient.POST('/api/v1/spaces', { body: { name: name.trim(), description: description.trim() || null } })), onSuccess: () => { setName(''); setDescription(''); queryClient.invalidateQueries({ queryKey: spacesKey }) } })
  const archive = useMutation({ mutationFn: async (id: string) => unwrap(await apiClient.DELETE('/api/v1/spaces/{id}', { params: { path: { id } } })), onSuccess: () => { setSelectedId(''); queryClient.invalidateQueries({ queryKey: spacesKey }); queryClient.invalidateQueries({ queryKey: archivedSpacesKey }) } })
  const restore = useMutation({ mutationFn: async (id: string) => unwrap(await apiClient.POST('/api/v1/spaces/{id}/restore', { params: { path: { id } } })), onSuccess: () => { queryClient.invalidateQueries({ queryKey: spacesKey }); queryClient.invalidateQueries({ queryKey: archivedSpacesKey }) } })
  const selected = spaces.data?.find((space) => space.id === selectedId)
  return <section><PageHeader title={text.pages.spaces} />
    <section className="admin-panel"><h2>Создать пространство</h2><form className="inline-form" onSubmit={(event) => { event.preventDefault(); if (name.trim()) create.mutate() }}><label>Название пространства<input value={name} onChange={(event) => setName(event.target.value)} /></label><label>Описание<input value={description} onChange={(event) => setDescription(event.target.value)} /></label><button type="submit" disabled={!name.trim() || create.isPending}>Создать</button></form>{create.isError && <ErrorNotice error={create.error} />}</section>
    <section className="admin-panel"><h2>Активные пространства</h2>{spaces.isPending ? <LoadingBlock /> : spaces.isError ? <ErrorNotice error={spaces.error} /> : <><div className="resource-selector"><label>Пространство<select value={selectedId} onChange={(event) => setSelectedId(event.target.value)}>{spaces.data.map((space) => <option value={space.id} key={space.id}>{space.name}</option>)}</select></label>{selected && <button type="button" onClick={() => archive.mutate(selected.id)} disabled={archive.isPending}>Архивировать</button>}</div>{selected && <SpaceEditor space={selected} />}</>}{archive.isError && <ErrorNotice error={archive.error} />}</section>
    {selected && <SpaceAccess space={selected} />}
    <section className="admin-panel"><h2>Архив пространств</h2>{archived.isPending ? <LoadingBlock /> : archived.isError ? <ErrorNotice error={archived.error} /> : archived.data.items.length ? <ul className="admin-list">{archived.data.items.map((item) => <li className="admin-list-item" key={item.id}><span>{item.name}</span><button type="button" onClick={() => restore.mutate(item.id)} disabled={restore.isPending}>Восстановить</button></li>)}</ul> : <p className="notice">Архив пуст.</p>}{restore.isError && <ErrorNotice error={restore.error} />}</section>
  </section>
}
