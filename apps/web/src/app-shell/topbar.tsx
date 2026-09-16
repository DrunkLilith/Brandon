import { forwardRef, useState } from 'react'

export const Topbar = forwardRef<HTMLButtonElement, { onLogout(): Promise<void> }>(function Topbar({ onLogout }, searchRef) {
  const [profileOpen, setProfileOpen] = useState(false)
  return <header className="topbar">
    <div className="breadcrumbs" aria-label="Хлебные крошки"><span>Пространство</span><span aria-hidden="true">/</span><strong>Обзор</strong></div>
    <button ref={searchRef} className="search-button" type="button" aria-label="Открыть поиск"><span>Поиск по знаниям</span><kbd>Ctrl K</kbd></button>
    <div className="profile-menu">
      <button className="profile-button" type="button" aria-label="Профиль" aria-expanded={profileOpen} onClick={() => setProfileOpen((open) => !open)}>A</button>
      {profileOpen && <div className="profile-popover"><button type="button" onClick={() => void onLogout()}>Выйти</button></div>}
    </div>
  </header>
})
