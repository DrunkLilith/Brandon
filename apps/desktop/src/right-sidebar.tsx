import type { VaultIndex } from './types'
import { useMemo } from 'react'
import { sourceStructure, type NoteLocation } from './note-navigation'
import { PropertiesEditor } from './properties'

export function RightSidebar({ activePath, source, onChange, index, onOpen, onNavigate, onTag }: {
  activePath: string | null
  source: string
  onChange: (source:string)=>void
  index: VaultIndex | null
  onOpen: (path: string, location?: NoteLocation) => void
  onNavigate: (location: NoteLocation) => void
  onTag: (tag: string) => void
}) {
  const document = useMemo(()=>index?.documents.find((item) => item.path === activePath),[index,activePath])
  const headings = useMemo(()=>sourceStructure(source).headings,[source])
  const backlinks = useMemo(()=>activePath && index ? index.documents.filter((item) => item.links.some((link) => link.resolvedPath === activePath)) : [],[index,activePath])

  return <aside className="right-sidebar">
    <PropertiesEditor key={activePath} source={source} onChange={onChange}/>
    <section><h3>Содержание <b>{headings.length}</b></h3>{headings.length ? <nav className="outline-list" aria-label="Содержание заметки">{headings.map((heading) => <button key={`${heading.slug}-${heading.line}`} title={`Строка ${heading.line}`} data-heading-slug={heading.slug} style={{ paddingLeft: `${8 + (heading.level - 1) * 10}px` }} onClick={() => onNavigate({ line: heading.line })}>{heading.text}</button>)}</nav> : <Empty label="Нет заголовков" />}</section>
    <section><h3>Обратные ссылки <b>{backlinks.length}</b></h3>{backlinks.length ? <nav className="backlink-list">{backlinks.map((item) => <button key={item.path} onClick={() => onOpen(item.path, { line: item.links.find((link) => link.resolvedPath === activePath)?.line || 1 })}><strong>{item.title}</strong><span>{item.path}</span></button>)}</nav> : <Empty label="На заметку пока не ссылаются" />}</section>
    <section><h3>Исходящие ссылки <b>{document?.links.length ?? 0}</b></h3>{document?.links.length ? <nav className="backlink-list">{document.links.map((link, index) => <button disabled={!link.resolvedPath} key={`${link.target}-${index}`} onClick={() => link.resolvedPath && onOpen(link.resolvedPath, { heading: link.heading, block: link.block })}><strong>{link.display ?? link.target}</strong><span>{link.resolvedPath ?? 'Не найдено'}</span></button>)}</nav> : <Empty label="Нет ссылок" />}</section>
    <section><h3>Теги</h3>{document?.tags.length ? <div className="tag-cloud">{document.tags.map((tag) => <button key={tag} onClick={() => onTag(tag)}>#{tag}</button>)}</div> : <Empty label="Нет тегов" />}</section>
  </aside>
}

function Empty({ label }: { label: string }) {
  return <p className="panel-empty">{label}</p>
}
