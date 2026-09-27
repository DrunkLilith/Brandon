import type { components } from '../api/generated'
import { useI18n } from '../i18n'
export function SearchResults({
  hits,
  onSelect,
}: {
  hits: readonly components['schemas']['SearchHit'][]
  onSelect(id: string): void
}) {
  const { messages: text } = useI18n()
  return hits.length ? (
    <ul className="admin-list">
      {hits.map((hit) => (
        <li key={hit.pageId}>
          <button onClick={() => onSelect(hit.pageId)}>{hit.title}</button>
          <p>{hit.snippet}</p>
        </li>
      ))}
    </ul>
  ) : (
    <p>{text.workspace.noResults}</p>
  )
}
