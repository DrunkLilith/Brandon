import { NavLink } from 'react-router-dom'
import { useI18n } from '../i18n'

export function Sidebar() {
  const { messages: text } = useI18n()
  return <aside className="sidebar">
    <div className="brand-row"><span className="brand-mark small" aria-hidden="true">K</span><strong>Knowledge</strong></div>
    <label className="space-switcher">{text.workspace.spaces}
      <select aria-label={text.workspace.spaces} defaultValue="workspace"><option value="workspace">{text.app.workspaceLabel}</option></select>
    </label>
    <nav aria-label={text.app.navigation}>
      <NavLink end to="/app">{text.workspace.overview}</NavLink>
    </nav>
    <section aria-label={text.workspace.tree} className="page-tree">
      <div className="section-heading"><h2>{text.workspace.tree}</h2><button type="button" disabled aria-label={text.workspace.create} title={text.workspace.create}>+</button></div>
      <p>{text.workspace.choose}</p>
    </section>
  </aside>
}
