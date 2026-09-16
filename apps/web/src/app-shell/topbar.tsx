import { useState } from 'react'

export function Topbar({ onLogout }: { onLogout(): Promise<void> }) {
  const [profileOpen, setProfileOpen] = useState(false)
  return <header className="topbar">
    <div className="breadcrumbs" aria-label="Хлебные крошки"><span>Пространство</span><span aria-hidden="true">/</span><strong>Обзор</strong></div>
    <button className="search-button" type="button" disabled aria-label="Поиск — доступно в следующей версии" title="Доступно в следующей версии"><span>Поиск по знаниям · скоро</span></button>
    <div className="profile-menu">
      <button className="profile-button" type="button" aria-label="Профиль" aria-expanded={profileOpen} onClick={() => setProfileOpen((open) => !open)}>A</button>
      {profileOpen && <div className="profile-popover"><button type="button" onClick={() => void onLogout()}>Выйти</button></div>}
    </div>
  </header>
}
