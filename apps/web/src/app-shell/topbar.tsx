import { useState } from 'react'
import { Link } from 'react-router-dom'
import type { components } from '../api/generated'
import { useI18n } from '../i18n'

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
  const { messages: text } = useI18n()
  const [profileOpen, setProfileOpen] = useState(false)
  return (
    <header className="topbar">
      <nav className="breadcrumbs" aria-label={text.app.breadcrumbs}>
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
          aria-label={text.app.profile}
          aria-expanded={profileOpen}
          onClick={() => setProfileOpen((open) => !open)}
        >
          A
        </button>
        {profileOpen && (
          <div className="profile-popover">
            <button type="button" onClick={() => void onLogout()}>
              {text.app.logout}
            </button>
          </div>
        )}
      </div>
    </header>
  )
}
