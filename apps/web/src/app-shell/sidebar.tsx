export function Sidebar() {
  return <aside className="sidebar">
    <div className="brand-row"><span className="brand-mark small" aria-hidden="true">K</span><strong>Knowledge</strong></div>
    <label className="space-switcher">Пространство
      <select aria-label="Пространство" defaultValue="workspace"><option value="workspace">Рабочее пространство</option></select>
    </label>
    <nav aria-label="Основная навигация">
      <a aria-current="page" href="/app">Обзор</a>
      <a href="/app/recent">Недавние</a>
      <a href="/app/favorites">Избранное</a>
    </nav>
    <section aria-label="Дерево страниц" className="page-tree">
      <div className="section-heading"><h2>Страницы</h2><button type="button" aria-label="Создать страницу">+</button></div>
      <p>Страницы появятся здесь после создания.</p>
    </section>
  </aside>
}
