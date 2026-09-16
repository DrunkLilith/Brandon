import { NavLink } from 'react-router-dom'

export function Sidebar() {
  return <aside className="sidebar">
    <div className="brand-row"><span className="brand-mark small" aria-hidden="true">K</span><strong>Knowledge</strong></div>
    <label className="space-switcher">Пространство
      <select aria-label="Пространство" defaultValue="workspace"><option value="workspace">Рабочее пространство</option></select>
    </label>
    <nav aria-label="Основная навигация">
      <NavLink end to="/app">Обзор</NavLink>
      <NavLink to="/app/recent">Недавние</NavLink>
      <NavLink to="/app/favorites">Избранное</NavLink>
    </nav>
    <section aria-label="Дерево страниц" className="page-tree">
      <div className="section-heading"><h2>Страницы</h2><button type="button" disabled aria-label="Создать страницу — доступно в следующей версии" title="Доступно в следующей версии">+</button></div>
      <p>Страницы появятся здесь после создания.</p>
    </section>
  </aside>
}
