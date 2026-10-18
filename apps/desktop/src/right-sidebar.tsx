import type { VaultIndex } from './types'

export function RightSidebar({ activePath, index, onOpen }: {
  activePath: string | null
  index: VaultIndex | null
  onOpen: (path: string) => void
}) {
  const document = index?.documents.find((item) => item.path === activePath)
  const backlinks = activePath && index ? index.documents.filter((item) => item.links.some((link) => link.resolvedPath === activePath)) : []

  return <aside className="right-sidebar">
    <section><h3>Свойства</h3>{document && Object.keys(document.properties).length > 0 ? <div className="property-list">{Object.entries(document.properties).map(([key, value]) => <div key={key}><span>{key}</span><code>{formatValue(value)}</code></div>)}</div> : <Empty label="Нет свойств YAML" />}</section>
    <section><h3>Содержание <b>{document?.headings.length ?? 0}</b></h3>{document?.headings.length ? <nav className="outline-list">{document.headings.map((heading, index) => <button key={`${heading.slug}-${index}`} style={{ paddingLeft: `${8 + (heading.level - 1) * 10}px` }}>{heading.text}</button>)}</nav> : <Empty label="Нет заголовков" />}</section>
    <section><h3>Обратные ссылки <b>{backlinks.length}</b></h3>{backlinks.length ? <nav className="backlink-list">{backlinks.map((item) => <button key={item.path} onClick={() => onOpen(item.path)}><strong>{item.title}</strong><span>{item.path}</span></button>)}</nav> : <Empty label="На заметку пока не ссылаются" />}</section>
    <section><h3>Исходящие ссылки <b>{document?.links.length ?? 0}</b></h3>{document?.links.length ? <nav className="backlink-list">{document.links.map((link, index) => <button disabled={!link.resolvedPath} key={`${link.target}-${index}`} onClick={() => link.resolvedPath && onOpen(link.resolvedPath)}><strong>{link.display ?? link.target}</strong><span>{link.resolvedPath ?? 'Не найдено'}</span></button>)}</nav> : <Empty label="Нет ссылок" />}</section>
    <section><h3>Теги</h3>{document?.tags.length ? <div className="tag-cloud">{document.tags.map((tag) => <span key={tag}>#{tag}</span>)}</div> : <Empty label="Нет тегов" />}</section>
  </aside>
}

function formatValue(value: unknown) {
  if (Array.isArray(value)) return value.join(', ')
  if (value && typeof value === 'object') return JSON.stringify(value)
  return String(value)
}

function Empty({ label }: { label: string }) {
  return <p className="panel-empty">{label}</p>
}
