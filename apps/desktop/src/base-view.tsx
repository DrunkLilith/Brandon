import { invoke } from '@tauri-apps/api/core'
import { useEffect, useMemo, useRef, useState } from 'react'
import { FileIcon, SearchIcon } from './icons'
import { useVirtualRows } from './virtual-rows'
import { compileBase, compareValues, noteRow, type BaseDefinition } from './query-service'
import { PropertyDialog, displayProperty, type PropertyInspection } from './properties'
import type { NoteDocument, SaveOutcome, VaultIndex } from './types'
import {compileTaskFormat,isWorkingFormat,type TaskFormatDefinition} from './working-formats'
import {TaskFormatView} from './task-format-view'
import type {CanvasTaskUpdater} from './canvas-task-ui'
const collator=new Intl.Collator('ru',{numeric:true})

export function BaseView({root,index,source,onChange,onOpen,onSaved,onUpdateTask,busyTaskId=null,locked=false,initialView}:{root:string;index:VaultIndex|null;source:string;onChange:(source:string)=>void;onOpen:(path:string)=>void;onSaved?:()=>void;onUpdateTask?:CanvasTaskUpdater;busyTaskId?:string|null;locked?:boolean;initialView?:string}){
  const [working,setWorking]=useState(false),[taskDefinition,setTaskDefinition]=useState<TaskFormatDefinition|null>(null)
  const [query,setQuery]=useState(''),[selected,setSelected]=useState(0),[override,setOverride]=useState<{property:string;direction:'ASC'|'DESC'}|null>(null),[definition,setDefinition]=useState<BaseDefinition|null>(null),[error,setError]=useState(''),[configOpen,setConfigOpen]=useState(false),[draft,setDraft]=useState(''),[target,setTarget]=useState<{document:NoteDocument;key:string;value:unknown}|null>(null),[busy,setBusy]=useState(false)
  const generation=useRef(0)
  const currentSource=useRef(source),configBase=useRef(source),alive=useRef(true);currentSource.current=source
  useEffect(()=>{alive.current=true;return()=>{alive.current=false}},[])
  useEffect(()=>{const request=++generation.current;setDefinition(null);setTaskDefinition(null);setError('');void invoke<unknown>('parse_structured_yaml',{contents:source}).then(value=>{if(request!==generation.current)return;const task=isWorkingFormat(value);setWorking(task);if(task)setTaskDefinition(compileTaskFormat(value));else setDefinition(compileBase(value))}).catch(error=>{if(request===generation.current)setError(String(error))});return()=>{generation.current++}},[source])
  const view=definition?.views[Math.min(selected,(definition?.views.length??1)-1)],columns=view?.columns??[]
  const sorting=useMemo(()=>override?[override]:view?.sort??[],[override,view])
  const rows=useMemo(()=>{
    if(!view||view.error)return []
    const normalized=query.trim().toLocaleLowerCase('ru'),result=(index?.documents??[]).filter(document=>Boolean(view.test(noteRow(document)))&&(!normalized||`${document.title} ${document.path} ${document.tags.join(' ')} ${Object.values(document.properties).map(displayProperty).join(' ')}`.toLocaleLowerCase('ru').includes(normalized)))
    result.sort((left,right)=>{const a=noteRow(left),b=noteRow(right);for(const item of sorting){const compared=compareValues(a.field(item.property),b.field(item.property),collator);if(compared)return compared*(item.direction==='DESC'?-1:1)}return collator.compare(left.path,right.path)})
    return result.slice(0,view.limit)
  },[index,query,view,sorting])
  const window=useVirtualRows(rows.length,47)
  const applyTaskDefinition=async()=>{
    if(locked||busy)return
    const submitted=draft,before=configBase.current;setBusy(true)
    try{const raw=await invoke<unknown>('parse_structured_yaml',{contents:submitted});compileTaskFormat(raw);if(!alive.current)return;if(currentSource.current!==before)throw Error('Источник изменился; ваш ввод оставлен в форме, но не записан.');onChange(submitted);setConfigOpen(false);setError('')}catch(reason){if(alive.current)setError(String(reason))}finally{if(alive.current)setBusy(false)}
  }
  const edit=async(path:string,key:string)=>{if(busy||locked)return;setBusy(true);try{
    const document=await invoke<NoteDocument>('read_note',{relativePath:path}),inspection=await invoke<PropertyInspection>('inspect_note_properties',{contents:document.content}),name=key.startsWith('note.')?key.slice(5):key
    if(inspection.error)throw new Error(inspection.error);if(Object.hasOwn(inspection.values,name)&&!inspection.editable[name])throw new Error('Вложенное или сложное значение доступно только в исходнике заметки')
    setTarget({document,key:name,value:inspection.values[name]})
  }catch(error){setError(String(error))}finally{setBusy(false)}}
  if(working)return <div className="base-view working-base"><div className="base-toolbar"><span>Связанные представления задач</span><button disabled={locked} onClick={()=>{configBase.current=source;setDraft(source);setConfigOpen(v=>!v)}}>Настроить источник .base</button></div>{error&&<p role="alert">{error}</p>}
    {configOpen&&<div className="base-config"><label>Исходное определение .base<textarea aria-label="Определение связанных форматов" value={draft} disabled={locked||busy} onChange={e=>setDraft(e.target.value)} rows={12}/></label><p>Фильтр и виды ссылаются на одну базу задач. Состояния задач сюда не копируются.</p><button disabled={locked||busy} onClick={()=>void applyTaskDefinition()}>Применить определение</button><button disabled={busy} onClick={()=>setConfigOpen(false)}>Отмена</button></div>}
    {taskDefinition?<TaskFormatView definition={taskDefinition} tasks={index?.tasks??[]} busyId={busyTaskId} onOpen={onOpen} onUpdate={onUpdateTask} locked={locked} initialView={initialView}/>:<p>Определение недоступно. Исходный файл не изменён; автоматической подмены заметками нет.</p>}
  </div>
  return <div className="base-view">
    <div className="base-toolbar"><label><SearchIcon/><input value={query} onChange={event=>setQuery(event.target.value)} placeholder="Фильтр записей…"/></label><span>{rows.length} записей</span>{definition&&<select aria-label="Представление базы" value={Math.min(selected,definition.views.length-1)} onChange={event=>{setSelected(Number(event.target.value));setOverride(null)}}>{definition.views.map((item,i)=><option key={i} value={i}>{item.name}</option>)}</select>}<button className="base-config-toggle" disabled={locked} onClick={()=>{configBase.current=source;setDraft(source);setConfigOpen(value=>!value)}}>Настроить .base</button></div>
    <div className="base-scope">Поднабор .base: Markdown-заметки · файлы-вложения не входят{override&&<><span> · временная сортировка</span><button onClick={()=>setOverride(null)}>Из файла</button></>}</div>
    {(error||view?.error)&&<div className="property-error" role="alert">{error||view?.error}</div>}
    {definition&&definition.warnings.length>0&&<details className="base-warnings"><summary>Не применены: {definition.warnings.length}. Исходник сохранён.</summary>{definition.warnings.map((warning,i)=><p key={i}>{warning}</p>)}</details>}
    {configOpen&&<div className="base-config"><label>Исходное определение .base<textarea aria-label="Определение базы YAML" value={draft} onChange={event=>setDraft(event.target.value)} rows={12}/></label><p>order — столбцы, filters — условия, sort — сортировка, limit — максимум строк. Неизвестные разделы остаются в файле.</p><button className="base-config-apply" disabled={locked} onClick={async()=>{try{const value=await invoke<unknown>('parse_structured_yaml',{contents:draft});compileBase(value);if(!alive.current)return;if(currentSource.current!==configBase.current)throw new Error("Определение изменилось извне. Ваш текст оставлен в форме; откройте актуальный файл перед применением.");onChange(draft);setOverride(null);setConfigOpen(false);setError('')}catch(error){setError(String(error))}}}>Применить определение</button><button onClick={()=>setConfigOpen(false)}>Отмена</button></div>}
    <div ref={window.ref} className="base-table-wrap" data-total-rows={rows.length}><table className="base-table"><thead><tr>{columns.map((column,i)=><th key={`${column}-${i}`}><button onClick={()=>setOverride({property:column,direction:sorting[0]?.property===column&&sorting[0].direction==='ASC'?'DESC':'ASC'})}>{definition?.labels[column]??definition?.labels[column.replace(/^note\./,'')]??({'file.name':'Имя','file.path':'Путь','file.tags':'Теги'}[column]??column)}{sorting[0]?.property===column&&<span>{sorting[0].direction==='DESC'?'↓':'↑'}</span>}</button></th>)}</tr></thead><tbody>{window.top>0&&<tr aria-hidden="true"><td colSpan={columns.length} style={{height:window.top,padding:0,border:0}}/></tr>}{rows.slice(window.start,window.end).map(document=><tr key={document.path} data-path={document.path} onDoubleClick={event=>{if(!(event.target as HTMLElement).closest('.base-cell-edit'))onOpen(document.path)}}>{columns.map((column,i)=><td key={`${column}-${i}`}>{column==='file.name'?<button onClick={()=>onOpen(document.path)}><FileIcon/><span>{noteRow(document).field(column) as string}</span></button>:column.startsWith('file.')?<span className={column==='file.path'?'base-path':''}>{displayProperty(noteRow(document).field(column))}</span>:<button className="base-cell-edit" data-property={column} disabled={busy||locked} title={`Изменить ${column}`} onClick={()=>void edit(document.path,column)}>{displayProperty(noteRow(document).field(column))}</button>}</td>)}</tr>)}{window.bottom>0&&<tr aria-hidden="true"><td colSpan={columns.length} style={{height:window.bottom,padding:0,border:0}}/></tr>}</tbody></table>{!rows.length&&<div className="base-empty">{!definition&&!error?'Читаю определение…':view?.error||error?'Представление недоступно. Проверьте определение .base.':'Нет заметок, подходящих под фильтр.'}</div>}</div>
    {target&&<PropertyDialog name={target.key} value={target.value} onClose={()=>setTarget(null)} onApply={async patch=>{if(locked)throw Error('Источник заблокирован');const outcome=await invoke<SaveOutcome>('patch_note_property',{root,relativePath:target.document.path,expectedFingerprint:target.document.fingerprint,patch});if(outcome.status==='conflict')throw new Error('Файл изменён извне. Правка не записана; закройте окно и откройте свойство заново.');onSaved?.()}}/>}
  </div>
}
