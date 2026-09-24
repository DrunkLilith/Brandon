import { useState } from 'react'
import { Link } from 'react-router-dom'
import type { components } from '../api/generated'
import { workspaceMessages as text } from '../pages/messages'

export function Topbar({
  onLogout,
  onSearch,
  space,
  page,
}: {
  onLogout(): Promise<void>
  onSearch(): void
  space?: components['schemas']['Space']
  page?: components['schemas']['Page']
}) {
  const [profileOpen, setProfileOpen] = useState(false)
  return (
    <header className="topbar">
      <nav className="breadcrumbs" aria-label="Хлебные крошки">
        <Link to="/app">{text.workspace.overview}</Link>
        {space && (
          <>
            <span aria-hidden="true">/</span>
            <Link to={`/app/spaces/${space.id}`}>{space.name}</Link>
          </>
        )}
        {page && <strong>{page.title}</strong>}
      </nav>
      <button className="search-button" type="button" onClick={onSearch}>
        <span>{text.workspace.search}</span>
        <kbd>Ctrl K</kbd>
      </button>
      <div className="profile-menu">
        <button
          className="profile-button"
          type="button"
          aria-label="Профиль"
          aria-expanded={profileOpen}
          onClick={() => setProfileOpen((open) => !open)}
        >
          A
        </button>
        {profileOpen && (
          <div className="profile-popover">
            <button type="button" onClick={() => void onLogout()}>
              Выйти
            </button>
          </div>
        )}
      </div>
    </header>
  )
}
