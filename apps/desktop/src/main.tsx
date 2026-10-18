import { Component, StrictMode, type ErrorInfo, type ReactNode } from 'react'
import { createRoot } from 'react-dom/client'
import { App } from './app'
import './styles.css'

document.addEventListener('contextmenu', (event) => event.preventDefault())

class FatalErrorBoundary extends Component<{ children: ReactNode }, { error: Error | null }> {
  state = { error: null as Error | null }

  static getDerivedStateFromError(error: Error) { return { error } }

  componentDidCatch(error: Error, info: ErrorInfo) {
    console.error('Brandon renderer crashed', error, info)
  }

  render() {
    if (!this.state.error) return this.props.children
    return <main className="fatal-screen">
      <div>
        <span>BRANDON</span>
        <h1>Интерфейс не загрузился</h1>
        <p>Ваши Markdown-файлы не затронуты. Перезапустите приложение; если ошибка повторится, пришлите этот текст:</p>
        <code>{this.state.error.message}</code>
        <button onClick={() => window.location.reload()}>Перезапустить интерфейс</button>
      </div>
    </main>
  }
}

createRoot(document.getElementById('root')!).render(
  <StrictMode>
    <FatalErrorBoundary><App /></FatalErrorBoundary>
  </StrictMode>,
)
