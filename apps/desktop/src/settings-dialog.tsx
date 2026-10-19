import { useEffect, useRef, useState } from 'react'
import { bindingError, effectiveShortcut, shortcutFromEvent, shortcutLabel, type Command } from './commands'
import { defaultSettings, type DesktopSettings } from './settings'

export function SettingsDialog({ settings, commands, onChange, onClose }: {
  settings: DesktopSettings; commands: Command[]; onChange: (settings: DesktopSettings) => void; onClose: () => void
}) {
  const [section, setSection] = useState<'editor' | 'hotkeys'>('editor')
  const [query, setQuery] = useState('')
  const [filter, setFilter] = useState('all')
  const [capture, setCapture] = useState<string | null>(null)
  const [error, setError] = useState('')
  const ref = useRef<HTMLDivElement>(null)
  useEffect(() => {
    const previous = document.activeElement as HTMLElement | null
    ref.current?.querySelector<HTMLButtonElement>('.settings-close')?.focus()
    return () => { if (previous?.isConnected) previous.focus() }
  }, [])
  function assign(id: string, binding: string | null | undefined) {
    const next = { ...settings.hotkeys }
    if (binding === undefined) delete next[id]
    else next[id] = binding
    const command = commands.find((command) => command.id === id)
    const issue = bindingError(id, binding === undefined ? command?.defaultShortcut ?? null : binding, commands, next)
    if (issue) { setError(issue); return }
    setError(''); setCapture(null); onChange({ ...settings, hotkeys: next })
  }
  const rows = commands.filter((command) => {
    const assigned = effectiveShortcut(command, settings.hotkeys)
    return `${command.label} ${command.id}`.toLocaleLowerCase('ru').includes(query.toLocaleLowerCase('ru'))
      && (filter === 'all' || filter === 'assigned' && assigned || filter === 'custom' && Object.hasOwn(settings.hotkeys, command.id) || filter === 'unassigned' && !assigned)
  })
  return <div className="settings-backdrop" onMouseDown={(event) => { if (event.target === event.currentTarget) onClose() }}>
    <div ref={ref} className="settings-dialog" role="dialog" aria-modal="true" aria-labelledby="settings-title" onKeyDown={(event) => {
      if (event.key === 'Escape') { event.stopPropagation(); if (capture) { setCapture(null); setError('') } else onClose(); return }
      if (event.key !== 'Tab') return
      const controls = Array.from(ref.current?.querySelectorAll<HTMLElement>('button:not(:disabled), input, select, [tabindex="0"]') ?? [])
      if (event.shiftKey && document.activeElement === controls[0]) { event.preventDefault(); controls.at(-1)?.focus() }
      else if (!event.shiftKey && document.activeElement === controls.at(-1)) { event.preventDefault(); controls[0]?.focus() }
    }}>
      <header><div><small>BRANDON</small><h2 id="settings-title">Настройки</h2></div><button className="settings-close" aria-label="Закрыть настройки" onClick={onClose}>×</button></header>
      <div className="settings-layout"><nav aria-label="Разделы настроек"><button className={section === 'editor' ? 'active' : ''} onClick={() => { setSection('editor'); setCapture(null); setError('') }}>Редактор</button><button className={section === 'hotkeys' ? 'active' : ''} onClick={() => setSection('hotkeys')}>Горячие клавиши</button></nav>
        <section className="settings-content">
          {section === 'editor' ? <>
            <h3>Режимы и отображение</h3>
            <label className="setting-row"><span><strong>Свойства в документе</strong><small>Сложный YAML всегда можно изменить в исходном Markdown.</small></span><select aria-label="Свойства в документе" value={settings.propertiesMode} onChange={event=>onChange({...settings,propertiesMode:event.target.value as DesktopSettings['propertiesMode']})}><option value="visible">Видимые</option><option value="hidden">Скрытые</option><option value="source">Исходный YAML</option></select></label>
            <label className="setting-row"><span><strong>Редактор Markdown</strong><small>Меняется сразу, без перезагрузки и потери undo.</small></span><select aria-label="Редактор Markdown" value={settings.editorMode} onChange={(event) => onChange({ ...settings, editorMode: event.target.value as DesktopSettings['editorMode'] })}><option value="visual">Визуальный</option><option value="source">Исходный Markdown</option></select></label>
            <label className="setting-row"><span><strong>Режим новых документов</strong><small>Применяется при открытии другой заметки.</small></span><select aria-label="Режим новых документов" value={settings.defaultViewMode} onChange={(event) => onChange({ ...settings, defaultViewMode: event.target.value as DesktopSettings['defaultViewMode'] })}><option value="edit">Редактор</option><option value="split">Редактор и чтение</option><option value="preview">Чтение</option></select></label>
            {([{ key: 'readableLineWidth', label: 'Ограничивать длину строки', description: 'Удобная ширина абзаца в редакторе и чтении.' }, { key: 'lineNumbers', label: 'Номера строк', description: 'Нумерация исходных строк, в том числе в visual mode.' }, { key: 'spellcheck', label: 'Проверять орфографию', description: 'Используется словарь системы; блоки кода исключены.' }] as const).map((item) => <label className="setting-row" key={item.key}><span><strong>{item.label}</strong><small>{item.description}</small></span><input type="checkbox" role="switch" aria-label={item.label} checked={settings[item.key]} onChange={(event) => onChange({ ...settings, [item.key]: event.target.checked })} /></label>)}
            <h3>Ссылки</h3>
            <h3>Ежедневные заметки и шаблоны</h3>
            {([['dailyFolder','Папка ежедневных заметок','Daily'],['dailyTemplate','Шаблон ежедневной заметки','Templates/Daily.md'],['templatesFolder','Папка шаблонов','Пусто: Templates и Шаблоны']] as const).map(([key,label,placeholder])=><label className="setting-row" key={key}><span><strong>{label}</strong><small>Относительный путь внутри текущего vault. Пустая папка заметок означает корень.</small></span><input aria-label={label} value={settings[key]} placeholder={placeholder} onChange={event=>onChange({...settings,[key]:event.target.value})}/></label>)}
            <label className="setting-row"><span><strong>Формат ежедневной заметки</strong></span><select aria-label="Формат ежедневной заметки" value={settings.dailyFormat} onChange={event=>onChange({...settings,dailyFormat:event.target.value as DesktopSettings['dailyFormat']})}>{['YYYY-MM-DD','DD-MM-YYYY','YYYY.MM.DD'].map(format=><option key={format}>{format}</option>)}</select></label>
            <label className="setting-row"><span><strong>Предпросмотр при наведении</strong><small>Только локальные заметки. Ctrl + клик открывает ссылку из редактора.</small></span><select aria-label="Предпросмотр при наведении" value={settings.previewModifier} onChange={(event) => onChange({ ...settings, previewModifier: event.target.value as DesktopSettings['previewModifier'] })}><option value="ctrl">С удержанием Ctrl</option><option value="none">Без модификатора</option></select></label>
          </> : <>
            <h3>Горячие клавиши</h3><p>Нажмите сочетание в поле назначения. Физические клавиши работают одинаково в RU/EN. Сочетания обычного ввода и буфера обмена защищены.</p>
            <input className="settings-search" aria-label="Поиск команды" placeholder="Поиск команды…" value={query} onChange={(event) => setQuery(event.target.value)} />
            <div className="hotkey-filters">{[['all', 'Все'], ['assigned', 'Назначенные'], ['custom', 'Мои'], ['unassigned', 'Не назначенные']].map(([id, label]) => <button key={id} className={filter === id ? 'active' : ''} onClick={() => setFilter(id)}>{label}</button>)}</div>
            {error && <div className="hotkey-error" role="alert">{error}</div>}
            <div className="hotkey-list">{rows.map((command) => <div className="hotkey-row" key={command.id}><span><strong>{command.label}</strong><small>{command.id}</small></span><div>
              {capture === command.id ? <input autoFocus className="hotkey-capture" readOnly aria-label={`Сочетание: ${command.label}`} placeholder="Нажмите сочетание…" onKeyDown={(event) => {
                if (event.key === 'Escape' || event.key === 'Tab') return
                event.preventDefault(); event.stopPropagation()
                const binding = shortcutFromEvent(event.nativeEvent)
                if (binding) assign(command.id, binding)
              }} /> : <button className="hotkey-binding" aria-label={`Назначить: ${command.label}`} onClick={() => { setCapture(command.id); setError('') }}>{shortcutLabel(effectiveShortcut(command, settings.hotkeys)) || 'Не назначено'}</button>}
              <button aria-label={`Снять: ${command.label}`} title="Снять назначение" onClick={() => assign(command.id, null)}>×</button><button aria-label={`По умолчанию: ${command.label}`} title="По умолчанию" onClick={() => assign(command.id, undefined)}>↺</button>
            </div></div>)}{!rows.length && <p>Команд по этому фильтру нет.</p>}</div>
          </>}
        </section>
      </div><footer><span>Локальные настройки приложения · схема v1</span><button onClick={() => { if (window.confirm('Сбросить настройки редактора и все собственные сочетания? Файлы не изменятся.')) { setCapture(null); setError(''); onChange(defaultSettings()) } }}>Сбросить настройки</button></footer>
    </div>
  </div>
}
