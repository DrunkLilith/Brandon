import {useEffect,useMemo,useRef,useState,type PointerEvent as ReactPointerEvent} from 'react'
import type {TaskMetadata,TaskPatch} from './types'
import {queryTasks,taskStatus} from './task-query'
import {civilDate,civilOrdinal,civilSchedule,shiftTaskSchedule,resizeTaskDuration,timelineTask,type TaskFormatDefinition} from './working-formats'
import {TaskDetailFields} from './task-detail-fields'
import {useVirtualRows} from './virtual-rows'

type Updater=(task:TaskMetadata,patch:TaskPatch)=>Promise<boolean>
type Shared={tasks:TaskMetadata[];busy:boolean;locked:boolean;onSelect:(id:string)=>void;onUpdate:Updater}
const statuses=['todo','doing','done'] as const
const labels={todo:'К выполнению',doing:'В работе',done:'Готово'}
const editable=(task:TaskMetadata,props:Pick<Shared,'busy'|'locked'>)=>task.managed&&Boolean(task.revision)&&!props.busy&&!props.locked
const today=()=>{const d=new Date();return`${d.getFullYear()}-${String(d.getMonth()+1).padStart(2,'0')}-${String(d.getDate()).padStart(2,'0')}`}
function TaskTable(props:Shared){
  const rows=useVirtualRows(props.tasks.length,54)
  return <div ref={rows.ref} className="task-format-scroll"><table className="task-format-table"><thead><tr><th>Задача</th><th>Статус</th><th>Проект</th><th>План</th><th>Дедлайн</th><th>Длительность</th></tr></thead><tbody>
    {rows.top>0&&<tr aria-hidden="true"><td colSpan={6} style={{height:rows.top,padding:0,border:0}}/></tr>}
    {props.tasks.slice(rows.start,rows.end).map(task=><tr key={task.id} data-task-id={task.id}><td><button onClick={()=>props.onSelect(task.id)}>{task.title}</button>{!task.managed&&<small>Markdown · только чтение</small>}</td><td><select aria-label={`Статус: ${task.title}`} disabled={!editable(task,props)} value={taskStatus(task)} onChange={e=>void props.onUpdate(task,{status:e.target.value as TaskMetadata['status']})}>{statuses.map(s=><option key={s} value={s}>{labels[s]}</option>)}</select></td><td>{task.project||'—'}</td><td>{task.scheduled||'Без плана'}</td><td>{task.due||'—'}</td><td>{task.durationMinutes?`${task.durationMinutes} мин`:'—'}</td></tr>)}
    {rows.bottom>0&&<tr aria-hidden="true"><td colSpan={6} style={{height:rows.bottom,padding:0,border:0}}/></tr>}
  </tbody></table>{!props.tasks.length&&<p>Нет подходящих задач.</p>}</div>
}
function KanbanColumn({status,...props}:Shared&{status:TaskMetadata['status']}){
  const tasks=props.tasks.filter(t=>taskStatus(t)===status),rows=useVirtualRows(tasks.length,126)
  // Capture the inspected revision at drag start, never substitute a newer row on drop.
  const drag=useRef<TaskMetadata|null>(null)
  useEffect(()=>()=>{drag.current=null;document.querySelectorAll('[data-task-drop-status].drop-over').forEach(n=>n.classList.remove('drop-over'))},[])
  useEffect(()=>{const g=drag.current;if(g&&(props.locked||props.busy||!props.tasks.some(t=>t.id===g.id&&t.revision===g.revision)))drag.current=null},[props.tasks,props.busy,props.locked])
  const start=(task:TaskMetadata,e:ReactPointerEvent<HTMLButtonElement>)=>{
    if(!editable(task,props)||e.button!==0)return
    e.preventDefault();e.currentTarget.setPointerCapture(e.pointerId);drag.current=task
  }
  const end=(e:ReactPointerEvent<HTMLButtonElement>)=>{
    const task=drag.current;drag.current=null;document.querySelectorAll('[data-task-drop-status].drop-over').forEach(n=>n.classList.remove('drop-over'));if(!task)return
    const target=document.elementFromPoint(e.clientX,e.clientY)?.closest<HTMLElement>('[data-task-drop-status]')?.dataset.taskDropStatus
    if(target&&statuses.includes(target as TaskMetadata['status'])&&target!==taskStatus(task))void props.onUpdate(task,{status:target as TaskMetadata['status']})
  }
  return <section className="task-format-kanban-column" data-task-drop-status={status}><header><strong>{labels[status]}</strong><span>{tasks.length}</span></header><div className="task-format-scroll" ref={rows.ref}><div style={{height:rows.top}}/>
    {tasks.slice(rows.start,rows.end).map(task=><article key={task.id} data-task-id={task.id}><button className="task-kanban-drag" aria-label={`Перенести задачу: ${task.title}`} disabled={!editable(task,props)} onPointerDown={e=>start(task,e)} onPointerMove={e=>{if(!drag.current)return;const target=document.elementFromPoint(e.clientX,e.clientY)?.closest<HTMLElement>('[data-task-drop-status]');document.querySelectorAll('[data-task-drop-status].drop-over').forEach(n=>n.classList.remove('drop-over'));target?.classList.add('drop-over')}} onPointerUp={end} onPointerCancel={()=>{drag.current=null;document.querySelectorAll('[data-task-drop-status].drop-over').forEach(n=>n.classList.remove('drop-over'))}} onLostPointerCapture={()=>{drag.current=null;document.querySelectorAll('[data-task-drop-status].drop-over').forEach(n=>n.classList.remove('drop-over'))}}>⠿</button><button className="task-format-title" onClick={()=>props.onSelect(task.id)}>{task.title}</button><small>{task.scheduled?'План: '+task.scheduled:'Без плановой даты'}{task.due?' · Дедлайн: '+task.due:''}</small><select aria-label={`Перенести статус: ${task.title}`} disabled={!editable(task,props)} value={status} onChange={e=>void props.onUpdate(task,{status:e.target.value as TaskMetadata['status']})}>{statuses.map(s=><option key={s} value={s}>{labels[s]}</option>)}</select></article>)}
    <div style={{height:rows.bottom}}/>{!tasks.length&&<p>Пока пусто</p>}
  </div></section>
}
type TimelineGesture={task:TaskMetadata;pointer:number;start:number;width:number;delta:number;resize:boolean}
function Timeline(props:Shared&{days:7|14|30|60;onError:(error:string)=>void}){
  const [start,setStart]=useState(today),[gesture,setGesture]=useState<TimelineGesture|null>(null),active=useRef<TimelineGesture|null>(null),rows=useVirtualRows(props.tasks.length,64),surface=useRef<HTMLDivElement>(null)
  const cancel=()=>{active.current=null;setGesture(null)}
  const navigate=(delta:number)=>{try{setStart(civilDate(civilOrdinal(start)+delta))}catch(reason){props.onError(String(reason))}}
  useEffect(()=>{const blur=()=>cancel();window.addEventListener('blur',blur);return()=>{window.removeEventListener('blur',blur);active.current=null}},[])
  useEffect(()=>{const g=active.current;if(g&&(props.locked||props.busy||!props.tasks.some(t=>t.id===g.task.id&&t.revision===g.task.revision))){cancel();props.onError('Задача или доступ изменились; жест отменён')}},[props.tasks,props.busy,props.locked])
  const begin=(task:TaskMetadata,e:ReactPointerEvent<HTMLButtonElement>,resize=false)=>{
    if(active.current||!editable(task,props)||e.button!==0)return
    const track=e.currentTarget.closest('.task-timeline-track')?.getBoundingClientRect();if(!track?.width)return
    e.preventDefault();e.stopPropagation();e.currentTarget.setPointerCapture(e.pointerId);const next={task,pointer:e.pointerId,start:e.clientX,width:track.width,delta:0,resize};active.current=next;setGesture(next)
  }
  const move=(e:ReactPointerEvent<HTMLButtonElement>)=>{const g=active.current;if(!g||g.pointer!==e.pointerId)return;const distance=(e.clientX-g.start)/g.width*props.days,delta=g.resize?Math.round(distance*1440/15)*15:Math.round(distance);const next={...g,delta};active.current=next;setGesture(next)}
  const finish=(e:ReactPointerEvent<HTMLButtonElement>)=>{
    const g=active.current;if(!g||g.pointer!==e.pointerId)return;cancel();if(!g.delta)return
    try{void props.onUpdate(g.task,g.resize?resizeTaskDuration(g.task,g.delta):shiftTaskSchedule(g.task,g.delta))}catch(reason){props.onError(String(reason))}
  }
  return <div ref={surface} className="task-format-timeline" onKeyDown={e=>{if(e.key==='Escape')cancel()}}><div className="task-timeline-navigation"><button disabled={Boolean(gesture)} onClick={()=>{navigate(-props.days)}}>←</button><input aria-label="Начало Timeline" type="date" value={start} disabled={Boolean(gesture)} onChange={e=>{try{civilOrdinal(e.target.value);setStart(e.target.value)}catch{}}}/><button disabled={Boolean(gesture)} onClick={()=>setStart(today())}>Сегодня</button><button disabled={Boolean(gesture)} onClick={()=>navigate(props.days)}>→</button><small>{props.days} дней · маркер ◆ — дедлайн, не конец плана</small></div>
    <div className="task-timeline-axis"><span>Задача / план</span><div>{Array.from({length:props.days},(_,i)=><small key={i}>{i%Math.ceil(props.days/10)===0?civilDate(civilOrdinal(start)+i).slice(5):''}</small>)}</div></div>
    <div ref={rows.ref} className="task-format-scroll"><div style={{height:rows.top}}/>{props.tasks.slice(rows.start,rows.end).map(task=>{
      let displayed=task;const g=gesture?.task.id===task.id?gesture:null
      if(g)try{displayed={...task,...(g.resize?resizeTaskDuration(task,g.delta):shiftTaskSchedule(task,g.delta))}}catch{/* Out-of-range preview is shown as a refusal on release, not written. */}
      const visual=(()=>{try{return timelineTask(displayed,start,props.days)}catch{return{bar:null,due:null,reason:'invalid' as const}}})(),bar=visual.bar,timed=Boolean(task.scheduled?.slice(10))
      return <div className="task-timeline-row" key={task.id} data-task-id={task.id}><button className="task-timeline-label" onClick={()=>props.onSelect(task.id)}><strong>{task.title}</strong><small>{displayed.scheduled||'Без плана'}{bar?.dateOnly?' · время не задано':''}{displayed.durationMinutes?` · ${displayed.durationMinutes} мин`:''}</small></button><div className="task-timeline-track" style={{backgroundSize:`${100/props.days}% 100%`}}>{bar?<div className={`task-timeline-interval ${bar.dateOnly?'date-only':''}`} style={{left:bar.left+'%',width:bar.width+'%'}}><button className="task-timeline-move" aria-label={`Перенести план: ${task.title}`} disabled={!editable(task,props)} title={displayed.scheduled??''} onPointerDown={e=>begin(task,e)} onPointerMove={move} onPointerUp={finish} onPointerCancel={cancel} onLostPointerCapture={cancel}/>{timed&&<button className="task-timeline-resize" aria-label={`Изменить длительность: ${task.title}`} title="Длительность, шаг 15 минут; дедлайн не меняется" disabled={!editable(task,props)} onPointerDown={e=>begin(task,e,true)} onPointerMove={move} onPointerUp={finish} onPointerCancel={cancel} onLostPointerCapture={cancel}/>}</div>:<span className="task-timeline-empty">{visual.reason==='unscheduled'?'Без плана — откройте задачу, чтобы запланировать' :visual.reason==='invalid'?'Некорректная дата — откройте исходник':'План вне окна'}</span>}{visual.due!==null&&<span className="task-timeline-due" title={`Дедлайн: ${task.due}`} style={{left:visual.due+'%'}}>◆</span>}</div></div>
    })}<div style={{height:rows.bottom}}/>{!props.tasks.length&&<p>Нет подходящих задач.</p>}</div>
    {gesture&&<p role="status">{gesture.resize?`Длительность: ${(gesture.task.durationMinutes??60)+gesture.delta} мин`:`Сдвиг плана: ${gesture.delta} дн.`} · Отпустите для сохранения, Esc — отменить. Дедлайн не меняется.</p>}
  </div>
}
function ScheduleFields({task,busy,onUpdate}:{task:TaskMetadata;busy:boolean;onUpdate:Updater}){
  const [baseline,setBaseline]=useState(task),[scheduled,setScheduled]=useState(task.scheduled??''),[due,setDue]=useState(task.due??''),[error,setError]=useState('')
  const dirty=scheduled!==(baseline.scheduled??'')||due!==(baseline.due??'')
  useEffect(()=>{if(!dirty){setBaseline(task);setScheduled(task.scheduled??'');setDue(task.due??'')}},[task,dirty])
  return <form className="task-format-schedule" onSubmit={e=>{e.preventDefault();try{if(scheduled)civilSchedule(scheduled);if(due)civilSchedule(due);setError('');void onUpdate(baseline,{scheduled:scheduled||null,due:due||null}).then(ok=>{if(ok)setBaseline({...baseline,scheduled:scheduled||null,due:due||null});else setError('Не сохранено. Загрузите актуальную версию при конфликте.')})}catch(reason){setError(String(reason))}}}><label>Плановая дата / время<input aria-label="Плановая дата связанной задачи" disabled={busy} value={scheduled} onChange={e=>setScheduled(e.target.value)} placeholder="2026-10-19 или 2026-10-19 09:00"/></label><label>Дедлайн<input aria-label="Дедлайн связанной задачи" disabled={busy} value={due} onChange={e=>setDue(e.target.value)} placeholder="Пусто — дедлайн не задан"/></label><small>Локальная календарная дата/время, без преобразования в UTC. План и дедлайн независимы.</small>{task.revision!==baseline.revision&&dirty&&<p role="alert">Задача изменилась; старая версия не перезапишет новую.</p>}{error&&<p role="alert">{error}</p>}<div><button disabled={!dirty||busy}>Сохранить даты</button><button type="button" disabled={busy} onClick={()=>{setBaseline(task);setScheduled(task.scheduled??'');setDue(task.due??'');setError('')}}>Загрузить актуальные</button></div></form>
}
export function TaskFormatView({definition,tasks,busyId,onOpen,onUpdate,locked=false,initialView}:{definition:TaskFormatDefinition;tasks:TaskMetadata[];busyId:string|null;onOpen:(path:string)=>void;onUpdate?:Updater;locked?:boolean;initialView?:string}){
  const [viewId,setViewId]=useState(initialView??definition.views[0].id),[query,setQuery]=useState(''),[imported,setImported]=useState(false),[selected,setSelected]=useState<string|null>(null),[error,setError]=useState(''),[localBusy,setLocalBusy]=useState(false),inFlight=useRef(false),alive=useRef(true)
  useEffect(()=>{alive.current=true;return()=>{alive.current=false}},[])
  const view=definition.views.find(v=>v.id===viewId)??definition.views[0]
  const rows=useMemo(()=>{const found=queryTasks(tasks.filter(t=>t.managed||imported),definition.filter,view.sort),needle=query.trim().toLocaleLowerCase('ru');return needle?found.filter(t=>(t.title+' '+t.path+' '+(t.project??'')+' '+t.tags.join(' ')).toLocaleLowerCase('ru').includes(needle)):found},[tasks,imported,definition,query,view])
  const current=tasks.find(t=>t.id===selected),busy=busyId!==null||localBusy
  const update:Updater=async(task,patch)=>{
    if(inFlight.current||busyId!==null||locked||!onUpdate||!task.managed||!task.revision)return false
    inFlight.current=true;setLocalBusy(true);setError('')
    try{const okay=await onUpdate(task,patch);if(!okay&&alive.current)setError('Правка не принята. Задача могла измениться; исходные данные не подменены.');return okay}catch(reason){if(alive.current)setError(String(reason));return false}finally{inFlight.current=false;if(alive.current)setLocalBusy(false)}
  }
  const props:Shared={tasks:rows,busy,locked:locked||!onUpdate,onSelect:setSelected,onUpdate:update}
  return <div className="task-format-view" data-task-format={view.layout}><header><select aria-label="Связанный формат задач" value={view.id} onChange={e=>{setViewId(e.target.value);setError('')}}>{definition.views.map(v=><option key={v.id} value={v.id}>{v.name}</option>)}</select><input aria-label="Поиск в связанном формате" placeholder="Найти задачу…" value={query} onChange={e=>setQuery(e.target.value)}/><span>{rows.length} задач</span><label><input type="checkbox" checked={imported} onChange={e=>setImported(e.target.checked)}/>Markdown-чекбоксы</label></header>
    <p className="task-format-scope">Одна база задач · TaskId и revision общие с календарём и карточками. {locked?'Источник/карточка заблокированы. ':''}Markdown-чекбоксы — только чтение. Это расширение Brandon, не стандартный task view Obsidian.</p>
    {definition.warnings.length>0&&<details><summary>Не применены поля: {definition.warnings.length}</summary>{definition.warnings.map(w=><p key={w}>{w}</p>)}</details>}{error&&<p role="alert">{error}</p>}
    <div className="task-format-body"><div className="task-format-results">{view.layout==='table'?<TaskTable {...props}/>:view.layout==='kanban'?<div className="task-format-kanban">{statuses.map(status=><KanbanColumn key={status} {...props} status={status}/>)}</div>:<Timeline key={view.id} {...props} days={view.windowDays} onError={setError}/>}</div>
      {selected&&<aside className="task-format-details"><header><strong>{current?.title??'Задача недоступна'}</strong><button aria-label="Закрыть связанную задачу" onClick={()=>setSelected(null)}>×</button></header>{current?<><button onClick={()=>onOpen(current.path)}>Открыть Markdown-описание</button><small>TaskId: {current.id}</small><TaskDetailFields task={current} busy={busy||locked||!onUpdate} onUpdate={update}/>{current.managed&&<ScheduleFields key={current.id} task={current} busy={busy||locked||!onUpdate} onUpdate={update}/>}</>:<p>Источник больше не содержит эту задачу. Автоматическая копия не создаётся.</p>}</aside>}
    </div>
  </div>
}
