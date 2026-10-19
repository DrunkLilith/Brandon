import { useMemo, useRef, useState, type FormEvent, type PointerEvent as ReactPointerEvent, type ReactNode } from 'react'
import { CalendarIcon, FileIcon, PlusIcon } from './icons'
import type { TaskMetadata } from './types'
import type {TaskPatch} from './types'
import {defaultTaskFilter,defaultTaskSort,queryTasks,taskStatus,type PlannerMode,type TaskFilter,type TaskSort} from './task-query'
import {TaskViewControls} from './task-views'
import {TaskDetailFields} from './task-detail-fields'
import {TaskBackupDialog} from './task-backup-dialog'
import {useVirtualRows} from './virtual-rows'

type PlannerViewProps = {
  root: string
  tasks: TaskMetadata[]
  busyId: string | null
  onOpen: (task: TaskMetadata) => void
  onCreate: (title: string, scheduled: string | null) => Promise<void>
  onUpdate: (task: TaskMetadata, patch: TaskPatch) => Promise<boolean>
}

const dayFormatter = new Intl.DateTimeFormat('ru-RU', { weekday: 'short' })
const dateFormatter = new Intl.DateTimeFormat('ru-RU', { day: 'numeric', month: 'short' })
const longDateFormatter = new Intl.DateTimeFormat('ru-RU', { day: 'numeric', month: 'long', year: 'numeric' })

function dateStamp(date: Date) {
  const pad = (value: number) => String(value).padStart(2, '0')
  return `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())}`
}

function startOfWeek(date: Date) {
  const value = new Date(date.getFullYear(), date.getMonth(), date.getDate(), 12)
  const weekday = value.getDay() || 7
  value.setDate(value.getDate() - weekday + 1)
  return value
}

function moveDays(date: Date, days: number) {
  const value = new Date(date)
  value.setDate(value.getDate() + days)
  return value
}

function scheduleDate(task: TaskMetadata) {
  return task.scheduled?.slice(0, 10) ?? null
}

function scheduleTime(task: TaskMetadata) {
  return task.scheduled?.length === 16 ? task.scheduled.slice(11) : null
}

function sourceName(path: string) {
  return path.split('/').at(-1)?.replace(/\.md$/i, '') ?? path
}

function scheduleOnDate(task: TaskMetadata, date: string) {
  const time = scheduleTime(task)
  return time ? `${date} ${time}` : date
}

function effectiveDeadline(task: TaskMetadata) {
  return task.due?.slice(0, 10) ?? null
}

function TaskCard({
  task,
  busy,
  selected,
  onSelect,
  onOpen,
  onUpdate,
  dragging,
  onPointerStart,
}: {
  task: TaskMetadata
  busy: boolean
  selected: boolean
  onSelect: () => void
  onOpen: () => void
  onUpdate: (patch: TaskPatch) => Promise<boolean>
  dragging: boolean
  onPointerStart: (taskId: string, event: ReactPointerEvent<HTMLElement>) => void
}) {
  return <article
    className={`planner-task ${task.completed ? 'completed' : ''} ${selected ? 'selected' : ''} ${dragging ? 'dragging' : ''}`}
    data-task-id={task.id}
    data-status={taskStatus(task)}
    onPointerDown={(event) => {
      if (task.managed&&!busy && event.button === 0 && !(event.target as HTMLElement).closest('input, button, a, label')) onPointerStart(task.id, event)
    }}
    onClick={onSelect}
    onDoubleClick={onOpen}
  >
    <div className="planner-task-main">
      <input
        type="checkbox"
        checked={task.completed}
        disabled={busy||!task.managed}
        aria-label={task.completed ? 'Вернуть задачу в работу' : 'Завершить задачу'}
        onClick={(event) => event.stopPropagation()}
        onChange={() => void onUpdate({ completed: !task.completed })}
      />
      <span>{task.title}</span>
      <i className="planner-drag-handle" title="Перетащить задачу">⠿</i>
    </div>
    <div className="planner-task-meta">
      {scheduleTime(task) && <time>{scheduleTime(task)}</time>}
      {task.durationMinutes && <span>{task.durationMinutes} мин</span>}
      {task.due && <span className="planner-due">до {task.due.slice(0, 10)}</span>}
      {task.project&&<span className="planner-task-project">{task.project}</span>}
      <span className="planner-source"><FileIcon />{sourceName(task.path)}</span>
    </div>
    {task.tags.length > 0 && <div className="planner-task-tags">{task.tags.slice(0, 3).map((tag) => <span key={tag}>#{tag}</span>)}</div>}
  </article>
}

function DropZone({
  children,
  className = '',
  dropId,
  pointerOver,
}: {
  children: ReactNode
  className?: string
  dropId: string
  pointerOver: boolean
}) {
  return <div
    className={`${className} ${pointerOver ? 'drop-active' : ''}`}
    data-planner-drop={dropId}
  >{children}</div>
}

type PointerDrag = {
  taskId: string
  pointerId: number
  startX: number
  startY: number
  source: HTMLElement
  active: boolean
}

export function PlannerView({ root,tasks, busyId, onOpen, onCreate, onUpdate }: PlannerViewProps) {
  const [mode, setMode] = useState<PlannerMode>('calendar')
  const [filter,setFilter]=useState<TaskFilter>(defaultTaskFilter),[sort,setSort]=useState<TaskSort>(defaultTaskSort),[viewError,setViewError]=useState(''),[backupOpen,setBackupOpen]=useState(false)
  const [weekOffset, setWeekOffset] = useState(0)
  const [title, setTitle] = useState('')
  const [captureDate, setCaptureDate] = useState('')
  const [showImported, setShowImported] = useState(false)
  const [selectedId, setSelectedId] = useState<string | null>(null)
  const [draggingId, setDraggingId] = useState<string | null>(null)
  const [pointerDrop, setPointerDrop] = useState<string | null>(null)
  const [dragPoint, setDragPoint] = useState<{ x: number; y: number } | null>(null)
  const [creating, setCreating] = useState(false)
  const pointerDrag = useRef<PointerDrag | null>(null)
  const suppressClick = useRef<string | null>(null)
  const today = dateStamp(new Date())
  const weekStart = moveDays(startOfWeek(new Date()), weekOffset * 7)
  const weekDays = useMemo(() => Array.from({ length: 7 }, (_, index) => moveDays(weekStart, index)), [weekStart])
  const importedCount = tasks.filter((task) => !task.managed).length
  const managedCount = tasks.length - importedCount
  const selection=useMemo(()=>{if(viewError)return {tasks:[],error:viewError};try{return {tasks:queryTasks(tasks.filter(task=>task.managed||showImported),filter,sort),error:''}}catch(error){return {tasks:[],error:String(error)}}},[tasks,showImported,filter,sort,viewError])
  const visibleTasks = selection.tasks
  const selected = visibleTasks.find((task) => task.id === selectedId) ?? null
  const taskById = (id: string) => visibleTasks.find((task) => task.id === id)
  const backlog = visibleTasks.filter((task) => !task.completed && !task.scheduled)
  const scheduled = visibleTasks.filter((task) => !task.completed && task.scheduled)
    .sort((left, right) => (left.scheduled ?? '').localeCompare(right.scheduled ?? ''))
  const completed = visibleTasks.filter((task) => task.completed)
  const calendarBacklog=visibleTasks.filter(task=>!task.scheduled)

  const submit = async (event: FormEvent) => {
    event.preventDefault()
    if (!title.trim() || creating) return
    setCreating(true)
    try {
      await onCreate(title.trim(), captureDate || null)
      setTitle('')
    } catch {
      // The application shell already surfaced the native error; keep the text
      // so a failed capture can be retried without retyping it.
    } finally {
      setCreating(false)
    }
  }

  const dropTask = (id: string, target: string | null) => {
    const task = taskById(id)
    if (!task || !task.managed||!target||busyId) return
    if(target==='kanban-todo'||target==='kanban-doing'||target==='kanban-done'){void onUpdate(task,{status:target.slice(7) as TaskMetadata['status']});return}
    if (target === 'calendar-backlog' || target === 'board-backlog') void onUpdate(task, { scheduled: null })
    else if (target === 'board-scheduled') void onUpdate(task, { scheduled: task.scheduled ?? today })
    else if (target === 'board-done') void onUpdate(task, { completed: true })
    else if (/^\d{4}-\d{2}-\d{2}$/.test(target)) void onUpdate(task, { scheduled: scheduleOnDate(task, target) })
  }

  const beginPointerDrag = (taskId: string, event: ReactPointerEvent<HTMLElement>) => {
    event.currentTarget.setPointerCapture(event.pointerId)
    pointerDrag.current = {
      taskId,
      pointerId: event.pointerId,
      startX: event.clientX,
      startY: event.clientY,
      source: event.currentTarget,
      active: false,
    }
  }
  const movePointerDrag = (event: ReactPointerEvent<HTMLElement>) => {
    const current = pointerDrag.current
    if (!current || current.pointerId !== event.pointerId) return
    if (!current.active && Math.hypot(event.clientX - current.startX, event.clientY - current.startY) < 6) return
    if (!current.active) {
      current.active = true
      setDraggingId(current.taskId)
    }
    event.preventDefault()
    setDragPoint({ x: event.clientX, y: event.clientY })
    const zone = document.elementFromPoint(event.clientX, event.clientY)?.closest<HTMLElement>('[data-planner-drop]')
    setPointerDrop(zone?.dataset.plannerDrop ?? null)
  }
  const finishPointerDrag = (event: ReactPointerEvent<HTMLElement>) => {
    const current = pointerDrag.current
    if (!current || current.pointerId !== event.pointerId) return
    if (current.source.hasPointerCapture(current.pointerId)) current.source.releasePointerCapture(current.pointerId)
    if (current.active) {
      event.preventDefault()
      const zone = document.elementFromPoint(event.clientX, event.clientY)?.closest<HTMLElement>('[data-planner-drop]')
      dropTask(current.taskId, zone?.dataset.plannerDrop ?? null)
      suppressClick.current = current.taskId
    }
    pointerDrag.current = null
    setDraggingId(null)
    setPointerDrop(null)
    setDragPoint(null)
  }
  const cancelPointerDrag = () => {
    pointerDrag.current = null
    setDraggingId(null)
    setPointerDrop(null)
    setDragPoint(null)
  }

  const renderTask = (task: TaskMetadata) => <TaskCard
    key={task.id}
    task={task}
    busy={busyId !== null}
    selected={selectedId === task.id}
    onSelect={() => {
      if (suppressClick.current === task.id) { suppressClick.current = null; return }
      setSelectedId(task.id)
    }}
    onOpen={() => onOpen(task)}
    onUpdate={(patch) => onUpdate(task, patch)}
    dragging={draggingId === task.id}
    onPointerStart={beginPointerDrag}
  />

  return <section className="planner-view" data-mode={mode} onPointerMove={movePointerDrag} onPointerUp={finishPointerDrag} onPointerCancel={cancelPointerDrag}>
    <header className="planner-header">
      <div>
        <p className="planner-eyebrow">Локальный планировщик</p>
        <h1>Задачи</h1>
        <span>{visibleTasks.filter((task) => !task.completed).length} в работе · {managedCount} задач Brandon · {backlog.length} без даты</span>
      </div>
      <div className="planner-header-actions">
        <button className="task-backup-open" onClick={()=>setBackupOpen(true)}>Backup / восстановление</button>
        {importedCount > 0 && <button className={`planner-import-filter ${showImported ? 'active' : ''}`} onClick={() => {
          setShowImported((value) => !value)
          if (showImported && selected && !selected.managed) setSelectedId(null)
        }}>Чекбоксы из заметок <b>{importedCount}</b></button>}
        <div className="planner-mode-switch" aria-label="Представление планировщика">
          <button className={mode === 'board' ? 'active' : ''} onClick={() => setMode('board')}>Доска</button>
          <button className={mode === 'list' ? 'active' : ''} onClick={() => setMode('list')}>Список</button>
          <button className={mode === 'kanban' ? 'active' : ''} onClick={() => setMode('kanban')}>Kanban</button>
          <button className={mode === 'calendar' ? 'active' : ''} onClick={() => setMode('calendar')}>Календарь</button>
        </div>
      </div>
    </header>

    <TaskViewControls root={root} mode={mode} filter={filter} sort={sort} onUnsupported={setViewError} onApply={(nextMode,nextFilter,nextSort)=>{setMode(nextMode);setFilter(nextFilter);setSort(nextSort);setViewError('')}}/>
    <div className="task-filters"><label>Поиск<input aria-label="Фильтр задач" value={filter.text} onChange={event=>setFilter({...filter,text:event.target.value})} placeholder="Название, проект, тег…"/></label><label>Статус<select aria-label="Фильтр статуса задач" value={filter.status} onChange={event=>setFilter({...filter,status:event.target.value as TaskFilter['status']})}>{[['all','Все'],['open','Незавершённые'],['todo','К выполнению'],['doing','В работе'],['done','Готово']].map(([value,label])=><option key={value} value={value}>{label}</option>)}</select></label><label>Проект<input aria-label="Фильтр проекта задач" list="task-projects" value={filter.project} onChange={event=>setFilter({...filter,project:event.target.value})} placeholder="Все проекты"/><datalist id="task-projects">{[...new Set(tasks.map(task=>task.project).filter(Boolean))].map(project=><option key={project} value={project??''}/>)}</datalist></label><label>Приоритет<select aria-label="Фильтр приоритета задач" value={filter.priority} onChange={event=>setFilter({...filter,priority:event.target.value})}>{[['','Все'],['urgent','Срочный'],['high','Высокий'],['normal','Обычный'],['low','Низкий'],['lowest','Минимальный']].map(([value,label])=><option key={value} value={value}>{label}</option>)}</select></label><label>Тег<input aria-label="Фильтр тега задач" value={filter.tag} onChange={event=>setFilter({...filter,tag:event.target.value})} placeholder="#тег"/></label><label>План<select aria-label="Фильтр плановой даты" value={filter.scheduled} onChange={event=>setFilter({...filter,scheduled:event.target.value as TaskFilter['scheduled']})}><option value="all">Все</option><option value="scheduled">С датой</option><option value="unscheduled">Без даты</option></select></label><label>Сортировка<select aria-label="Сортировка задач" value={sort.property} onChange={event=>setSort({...sort,property:event.target.value as TaskSort['property']})}>{[['scheduled','Плановая дата'],['due','Дедлайн'],['title','Название'],['status','Статус'],['project','Проект'],['priority','Приоритет'],['durationMinutes','Длительность']].map(([value,label])=><option key={value} value={value}>{label}</option>)}</select></label><button aria-label="Направление сортировки задач" onClick={()=>setSort({...sort,direction:sort.direction==='ASC'?'DESC':'ASC'})}>{sort.direction==='ASC'?'↑':'↓'}</button><details><summary>Условие</summary><input aria-label="Выражение фильтра задач" value={filter.expression} onChange={event=>setFilter({...filter,expression:event.target.value})} placeholder={'task.project == "Учёба"'}/><small>Только декларативные task.* условия. JavaScript не исполняется.</small></details><span className="task-result-count">{visibleTasks.length} из {managedCount+(showImported?importedCount:0)}</span></div>
    {selection.error&&<p className="task-error" role="alert">{selection.error}. Результаты скрыты; задачи не изменены.</p>}

    <form className="planner-capture" onSubmit={(event) => void submit(event)}>
      <PlusIcon />
      <input value={title} onChange={(event) => setTitle(event.target.value)} placeholder="Что нужно сделать?" aria-label="Название новой задачи" />
      <label><CalendarIcon /><input type="date" value={captureDate} onChange={(event) => setCaptureDate(event.target.value)} aria-label="Дата новой задачи" /></label>
      <button type="submit" disabled={!title.trim() || creating||busyId!==null}>{creating ? 'Добавляю…' : 'Добавить'}</button>
    </form>

    <div className={`planner-body ${selected ? 'with-detail' : ''}`}>
      {mode==='list'?<TaskList tasks={visibleTasks} busy={busyId!==null} selectedId={selectedId} onSelect={setSelectedId} onOpen={onOpen} onUpdate={onUpdate}/>:mode==='kanban'?<div className="planner-board planner-kanban">{(['todo','doing','done'] as const).map((status,i)=>{const items=visibleTasks.filter(task=>taskStatus(task)===status);return <DropZone key={status} className="planner-column" dropId={`kanban-${status}`} pointerOver={pointerDrop===`kanban-${status}`}><div className="planner-column-head"><div><i className={['backlog','scheduled','done'][i]}/><strong>{['К выполнению','В работе','Готово'][i]}</strong></div><span>{items.length}</span></div><p>Перенос меняет только статус, не плановую дату и не дедлайн.</p><div className="planner-task-list"><TaskStack tasks={items} render={renderTask}/>{!items.length&&<EmptyColumn text="Перетащите задачу сюда"/>}</div></DropZone>})}</div>:mode === 'board' ? <div className="planner-board">
        <DropZone className="planner-column" dropId="board-backlog" pointerOver={pointerDrop === 'board-backlog'}>
          <div className="planner-column-head"><div><i className="backlog" /><strong>Без даты</strong></div><span>{backlog.length}</span></div>
          <p>Задачи, которые ещё не заняли место в календаре.</p>
          <div className="planner-task-list"><TaskStack tasks={backlog} render={renderTask}/>{backlog.length === 0 && <EmptyColumn text="Перетащите сюда, чтобы снять дату" />}</div>
        </DropZone>
        <DropZone className="planner-column" dropId="board-scheduled" pointerOver={pointerDrop === 'board-scheduled'}>
          <div className="planner-column-head"><div><i className="scheduled" /><strong>Запланировано</strong></div><span>{scheduled.length}</span></div>
          <p>Работа, для которой уже выбрана дата.</p>
          <div className="planner-task-list"><TaskStack tasks={scheduled} render={renderTask}/>{scheduled.length === 0 && <EmptyColumn text="Перетащите задачу — назначим сегодня" />}</div>
        </DropZone>
        <DropZone className="planner-column" dropId="board-done" pointerOver={pointerDrop === 'board-done'}>
          <div className="planner-column-head"><div><i className="done" /><strong>Готово</strong></div><span>{completed.length}</span></div>
          <p>Статус хранится у той же задачи в канонической базе.</p>
          <div className="planner-task-list"><TaskStack tasks={completed} render={renderTask}/>{completed.length === 0 && <EmptyColumn text="Завершённые задачи появятся здесь" />}</div>
        </DropZone>
      </div> : <div className="planner-calendar">
        <DropZone className="planner-backlog" dropId="calendar-backlog" pointerOver={pointerDrop === 'calendar-backlog'}>
          <div className="planner-backlog-head"><div><strong>Без даты</strong><span>Можно планировать позже</span></div><b>{calendarBacklog.length}</b></div>
          <div className="planner-task-list"><TaskStack tasks={calendarBacklog} render={renderTask}/>{calendarBacklog.length === 0 && <EmptyColumn text="Все задачи разобраны" />}</div>
        </DropZone>
        <div className="planner-week-wrap">
          <div className="planner-week-toolbar">
            <div>
              <button aria-label="Предыдущая неделя" onClick={() => setWeekOffset((value) => value - 1)}>‹</button>
              <button className="today" onClick={() => setWeekOffset(0)}>Сегодня</button>
              <button aria-label="Следующая неделя" onClick={() => setWeekOffset((value) => value + 1)}>›</button>
            </div>
            <strong>{longDateFormatter.format(weekStart)} — {longDateFormatter.format(weekDays[6])}</strong>
          </div>
          <div className="planner-week">
            {weekDays.map((day) => {
              const stamp = dateStamp(day)
              const dayTasks = visibleTasks.filter((task) => scheduleDate(task) === stamp)
              return <DropZone key={stamp} className={`planner-day ${stamp === today ? 'is-today' : ''}`} dropId={stamp} pointerOver={pointerDrop === stamp}>
                <div className="planner-day-head"><span>{dayFormatter.format(day).replace('.', '')}</span><strong>{dateFormatter.format(day).replace('.', '')}</strong>{stamp === today && <i>сегодня</i>}</div>
                <div className="planner-day-tasks"><TaskStack tasks={dayTasks} render={renderTask}/></div>
              </DropZone>
            })}
          </div>
        </div>
      </div>}

      {selected && <aside className="planner-detail">
        <button className="planner-detail-close" aria-label="Закрыть сведения" onClick={() => setSelectedId(null)}>×</button>
        <p className="planner-eyebrow">{selected.managed?'Задача Brandon · описание в Markdown':'Обычный Markdown-чекбокс'}</p>
        <h2>{selected.title}</h2>
        <label className="planner-detail-check"><input type="checkbox" checked={selected.completed} disabled={busyId !== null||!selected.managed} onChange={() => void onUpdate(selected, { completed: !selected.completed })} />{selected.completed ? 'Завершена' : 'В работе'}</label>
        <TaskDetailFields key={selected.id} task={selected} busy={busyId!==null} onUpdate={onUpdate}/>
        <label className="planner-detail-date"><span>Дата работы</span><input type="date" value={scheduleDate(selected) ?? ''} disabled={busyId !== null||!selected.managed} onChange={(event) => void onUpdate(selected, { scheduled: event.target.value ? scheduleOnDate(selected, event.target.value) : null })} /></label>
        <label className="planner-detail-date planner-deadline-field"><span>Дедлайн</span><input type="date" value={selected.due?.slice(0, 10) ?? ''} disabled={busyId !== null||!selected.managed} onChange={(event) => {
          const due = event.target.value
          void onUpdate(selected, due ? { due } : { due: null })
        }} /></label>
        <p className="planner-deadline-note">{selected.due
          ? `Дедлайн ${selected.due.slice(0, 10)}. Плановая дата меняется отдельно.`
          : 'Дедлайн не задан. Плановая дата не считается дедлайном.'}</p>
        <dl>
          <div><dt>Запланировано</dt><dd>{selected.scheduled ?? 'Без даты'}</dd></div>
          <div><dt>Текущий дедлайн</dt><dd>{effectiveDeadline(selected) ?? 'Не задан'}</dd></div>
          <div><dt>Длительность</dt><dd>{selected.durationMinutes ? `${selected.durationMinutes} мин` : 'Не задана'}</dd></div>
          <div><dt>Источник</dt><dd>{selected.path}, строка {selected.line}</dd></div>
        </dl>
        {selected.tags.length > 0 && <div className="planner-detail-tags">{selected.tags.map((tag) => <span key={tag}>#{tag}</span>)}</div>}
        <div className="planner-detail-actions">
          <button className="primary" onClick={() => onOpen(selected)}><FileIcon />Открыть заметку на задаче</button>
          {selected.managed&&selected.scheduled && <button disabled={busyId!==null} onClick={() => void onUpdate(selected, { scheduled: null })}>Убрать из календаря</button>}
        </div>
        <p className="planner-detail-hint">Описание, ссылки и вложения — в Markdown. Статус и операционные поля управляемой задачи — в task DB; сохраняйте её вместе с файлами через backup.</p>
      </aside>}
    </div>
    {draggingId && dragPoint && <div className="planner-drag-preview" style={{ left: dragPoint.x + 14, top: dragPoint.y + 14 }}><span>Переместить</span><strong>{taskById(draggingId)?.title}</strong></div>}
    {backupOpen&&<TaskBackupDialog root={root} onClose={()=>setBackupOpen(false)}/>}
  </section>
}

function EmptyColumn({ text }: { text: string }) {
  return <div className="planner-empty-column"><span>＋</span><p>{text}</p></div>
}

function TaskStack({tasks,render}:{tasks:TaskMetadata[];render:(task:TaskMetadata)=>ReactNode}){
  const window=useVirtualRows(tasks.length,124,true)
  return <div ref={window.ref} className="planner-task-stack" data-total-rows={tasks.length}><div aria-hidden="true" style={{height:window.top}}/>{tasks.slice(window.start,window.end).map(task=><div className="planner-task-slot" key={task.id}>{render(task)}</div>)}<div aria-hidden="true" style={{height:window.bottom}}/></div>
}
function TaskList({tasks,busy,selectedId,onSelect,onOpen,onUpdate}:{tasks:TaskMetadata[];busy:boolean;selectedId:string|null;onSelect:(id:string)=>void;onOpen:(task:TaskMetadata)=>void;onUpdate:(task:TaskMetadata,patch:TaskPatch)=>Promise<boolean>}){
  const window=useVirtualRows(tasks.length,48)
  return <div ref={window.ref} className="task-list-wrap" data-total-rows={tasks.length}><table className="task-list-table"><thead><tr>{['','Задача','Статус','Проект','Приоритет','План','Дедлайн','Мин.'].map((label,i)=><th key={i}>{label}</th>)}</tr></thead><tbody>{window.top>0&&<tr aria-hidden="true"><td colSpan={8} style={{height:window.top,padding:0,border:0}}/></tr>}{tasks.slice(window.start,window.end).map(task=><tr data-task-id={task.id} data-status={taskStatus(task)} className={selectedId===task.id?'selected':''} key={task.id} onDoubleClick={()=>onOpen(task)}><td><input type="checkbox" aria-label={`Завершить: ${task.title}`} checked={task.completed} disabled={busy||!task.managed} onChange={()=>void onUpdate(task,{completed:!task.completed})}/></td><td><button onClick={()=>onSelect(task.id)} title={task.path}>{task.title}</button></td><td>{({todo:'К выполнению',doing:'В работе',done:'Готово'})[taskStatus(task)]}</td><td>{task.project??'—'}</td><td>{({urgent:'Срочный',high:'Высокий',normal:'Обычный',low:'Низкий',lowest:'Минимальный'})[task.priority??'normal']??'Обычный'}</td><td>{task.scheduled??'—'}</td><td>{task.due??'—'}</td><td>{task.durationMinutes??'—'}</td></tr>)}{window.bottom>0&&<tr aria-hidden="true"><td colSpan={8} style={{height:window.bottom,padding:0,border:0}}/></tr>}</tbody></table>{!tasks.length&&<div className="base-empty">Нет задач по выбранному фильтру.</div>}</div>
}
