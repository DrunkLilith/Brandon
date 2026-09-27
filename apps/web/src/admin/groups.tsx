import { useInfiniteQuery, useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { useState, type FormEvent } from 'react'

import { apiClient, unwrap, unwrapArray } from '../api/client'
import type { components } from '../api/generated'
import { useI18n } from '../i18n'
import { ErrorNotice, LoadingBlock, PageHeader } from './shared'

type Group = components['schemas']['Group']
type User = components['schemas']['User']
const groupsKey = ['admin', 'groups'] as const
const archivedGroupsKey = ['admin', 'groups', 'archived'] as const

function GroupCard({ group, users }: { group: Group; users: readonly User[] }) {
  const { messages: text } = useI18n()
  const queryClient = useQueryClient()
  const [search, setSearch] = useState('')
  const [name, setName] = useState(group.name)
  const [description, setDescription] = useState(group.description ?? '')
  const invalidate = () => queryClient.invalidateQueries({ queryKey: groupsKey })
  const membership = useMutation({
    mutationFn: async ({ userId, remove }: { userId: string; remove: boolean }) => remove
      ? unwrap(await apiClient.DELETE('/api/v1/groups/{group_id}/members/{user_id}', { params: { path: { group_id: group.id, user_id: userId } } }))
      : unwrap(await apiClient.PUT('/api/v1/groups/{group_id}/members/{user_id}', { params: { path: { group_id: group.id, user_id: userId } } })),
    onSuccess: invalidate,
  })
  const update = useMutation({
    mutationFn: async () => unwrap(await apiClient.PATCH('/api/v1/groups/{id}', { params: { path: { id: group.id } }, body: { name: name.trim(), description: description.trim() || null } })),
    onSuccess: invalidate,
  })
  const archive = useMutation({
    mutationFn: async () => unwrap(await apiClient.DELETE('/api/v1/groups/{id}', { params: { path: { id: group.id } } })),
    onSuccess: () => { invalidate(); queryClient.invalidateQueries({ queryKey: archivedGroupsKey }) },
  })
  const activeUsers = users.filter((user) => user.status === 'active')
  const members = activeUsers.filter((user) => group.memberIds.includes(user.id))
  const candidates = activeUsers.filter((user) => !group.memberIds.includes(user.id) && `${user.displayName} ${user.email ?? ''}`.toLowerCase().includes(search.trim().toLowerCase()))
  return <section className="admin-panel" role="region" aria-label={text.groups.groupFor(group.name)}>
    <div className="resource-heading"><div><h2>{group.name}</h2><p>{group.description || text.common.noDescription}</p></div><button type="button" onClick={() => archive.mutate()} disabled={archive.isPending}>{text.common.archive}</button></div>
    <details><summary>{text.groups.edit}</summary><div className="inline-form"><label>{text.groups.name}<input value={name} onChange={(event) => setName(event.target.value)} /></label><label>{text.common.description}<input value={description} onChange={(event) => setDescription(event.target.value)} /></label><button type="button" onClick={() => update.mutate()} disabled={!name.trim() || update.isPending}>{text.groups.save}</button></div></details>
    <h3>{text.groups.members}</h3>
    <ul className="admin-list">{members.map((member) => <li className="admin-list-item" key={member.id}><span>{member.displayName}</span><button type="button" aria-label={text.groups.removeMember(member.displayName)} onClick={() => membership.mutate({ userId: member.id, remove: true })} disabled={membership.isPending}>{text.common.remove}</button></li>)}</ul>
    <label className="search-field">{text.groups.activeUserSearch}<input type="search" value={search} onChange={(event) => setSearch(event.target.value)} /></label>
    {search.trim() && <ul className="admin-list search-results">{candidates.map((candidate) => <li className="admin-list-item" key={candidate.id}><span>{candidate.displayName}</span><button type="button" aria-label={text.groups.addMember(candidate.displayName)} onClick={() => membership.mutate({ userId: candidate.id, remove: false })} disabled={membership.isPending}>{text.groups.add}</button></li>)}</ul>}
    {(membership.isError || update.isError || archive.isError) && <ErrorNotice error={membership.error ?? update.error ?? archive.error} />}
  </section>
}

export function GroupsPage() {
  const { messages: text } = useI18n()
  const queryClient = useQueryClient()
  const [name, setName] = useState('')
  const [description, setDescription] = useState('')
  const groups = useQuery({ queryKey: groupsKey, queryFn: async () => unwrapArray(await apiClient.GET('/api/v1/groups')) })
  const users = useQuery({ queryKey: ['admin', 'users'], queryFn: async () => unwrapArray(await apiClient.GET('/api/v1/users')) })
  const archived = useInfiniteQuery({
    queryKey: archivedGroupsKey,
    initialPageParam: undefined as string | undefined,
    queryFn: async ({ pageParam }) => unwrap(await apiClient.GET('/api/v1/groups/archived', { params: { query: { limit: 100, after: pageParam } } })),
    getNextPageParam: (lastPage) => lastPage.nextCursor ?? undefined,
  })
  const create = useMutation({
    mutationFn: async () => unwrap(await apiClient.POST('/api/v1/groups', { body: { name: name.trim(), description: description.trim() || null } })),
    onSuccess: () => { setName(''); setDescription(''); queryClient.invalidateQueries({ queryKey: groupsKey }) },
  })
  const restore = useMutation({
    mutationFn: async (id: string) => unwrap(await apiClient.POST('/api/v1/groups/{id}/restore', { params: { path: { id } } })),
    onSuccess: () => { queryClient.invalidateQueries({ queryKey: groupsKey }); queryClient.invalidateQueries({ queryKey: archivedGroupsKey }) },
  })
  const submit = (event: FormEvent) => { event.preventDefault(); if (name.trim()) create.mutate() }
  return <section><PageHeader title={text.pages.groups} />
    <section className="admin-panel"><h2>{text.groups.create}</h2><form className="inline-form" onSubmit={submit}><label>{text.groups.nameLabel}<input value={name} onChange={(event) => setName(event.target.value)} /></label><label>{text.common.description}<input value={description} onChange={(event) => setDescription(event.target.value)} /></label><button type="submit" disabled={!name.trim() || create.isPending}>{text.common.create}</button></form>{create.isError && <ErrorNotice error={create.error} />}</section>
    {groups.isPending || users.isPending ? <LoadingBlock /> : groups.isError || users.isError ? <ErrorNotice error={groups.error ?? users.error} /> : groups.data.map((group) => <GroupCard key={group.id} group={group} users={users.data} />)}
    <section className="admin-panel"><h2>{text.groups.archive}</h2>{archived.isPending ? <LoadingBlock /> : archived.isError ? <ErrorNotice error={archived.error} /> : archived.data.pages.some((page) => page.items.length) ? <><ul className="admin-list">{archived.data.pages.flatMap((page) => page.items).map((item) => <li className="admin-list-item" key={item.id}><span>{item.name}</span><button type="button" onClick={() => restore.mutate(item.id)} disabled={restore.isPending}>{text.common.restore}</button></li>)}</ul>{archived.hasNextPage && <button type="button" onClick={() => archived.fetchNextPage()} disabled={archived.isFetchingNextPage}>{text.groups.loadMore}</button>}</> : <p className="notice">{text.common.emptyArchive}</p>}{restore.isError && <ErrorNotice error={restore.error} />}</section>
  </section>
}
