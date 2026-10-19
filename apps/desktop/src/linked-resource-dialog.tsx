import {useEffect,useRef,useState} from 'react'
import {invoke} from '@tauri-apps/api/core'
import {listen} from '@tauri-apps/api/event'
import ReactMarkdown from 'react-markdown'
import remarkGfm from 'remark-gfm'
import type {NoteDocument,SaveOutcome,VaultIndex} from './types'
import type {CanvasTaskUpdater} from './canvas-task-ui'
import {WorkspaceDialog} from './workspace-dialogs'
import {BaseView} from './base-view'
import {changedRasterPaths} from './canvas-raster'
import {useCrashDraft} from './workspace-state'

export function LinkedResourceDialog({root,path,pane,index,locked,available,onUpdateTask,busyTaskId,onOpen,onClose}:{root:string;path:string;pane:'main'|'side';index:VaultIndex|null;locked:boolean;available:boolean;onUpdateTask?:CanvasTaskUpdater;busyTaskId:string|null;onOpen:(path:string)=>void;onClose:()=>void}){
  const [document,setDocument]=useState<NoteDocument|null>(null),[text,setText]=useState(''),[edit,setEdit]=useState(false),[error,setError]=useState(''),[busy,setBusy]=useState(false),[external,setExternal]=useState<NoteDocument|'unavailable'|null>(null)
  const current=useRef({document,text,locked,available});current.current={document,text,locked,available}
  const mounted=useRef(false),writing=useRef(false),sequence=useRef(0)
  const dirty=Boolean(document&&text!==document.content),readonly=locked||!available
  const {acknowledge}=useCrashDraft(root,pane==='side'?'linked-side':'linked-main',path,text,document?.content??'',document?.fingerprint,Boolean(document),setError)
  useEffect(()=>{
    mounted.current=true;let disposed=false,stop:(()=>void)|undefined,timer=0
    const read=async(initial=false)=>{const ticket=++sequence.current;try{const next=await invoke<NoteDocument>('read_linked_resource',{root,path});if(disposed||ticket!==sequence.current)return;if(initial){setDocument(next);setText(next.content);setExternal(null);setError('')}else if(current.current.document?.fingerprint!==next.fingerprint)setExternal(next);else setExternal(null)}catch(reason){if(!disposed){if(initial)setError(String(reason));else setExternal('unavailable')}}}
    void read(true)
    void listen<string[]>('vault-changed',({payload})=>{if(!disposed&&changedRasterPaths(root,payload.length?payload:null,[path],true).length){window.clearTimeout(timer);timer=window.setTimeout(()=>void read(),180)}}).then(unlisten=>{if(disposed)unlisten();else stop=unlisten}).catch(reason=>{if(!disposed)setError(`Слежение недоступно; сохранение всё равно проверит источник: ${String(reason)}`)})
    return()=>{disposed=true;mounted.current=false;sequence.current++;window.clearTimeout(timer);stop?.()}
  },[root,path])
  const change=(next:string,strict=false)=>{if(current.current.locked||!current.current.available||writing.current){if(strict)throw Error('Ссылка или доступ изменились; ввод оставлен в форме');return}if(new TextEncoder().encode(next).length>2*1024*1024){if(strict)throw Error('Правка превышает 2 МиБ; исходник не обрезан');setError('Правка превышает 2 МиБ; вставка не применена');return}setText(next)}
  const save=async()=>{
    const latest=current.current;if(writing.current||!latest.document||latest.locked||!latest.available)return
    writing.current=true;setBusy(true);setError('')
    try{const result=await invoke<SaveOutcome>('save_linked_resource',{root,path,expected:latest.document.fingerprint,contents:latest.text});if(!mounted.current)return
      if(result.status==='conflict'){setExternal(result.disk);setError('Источник изменился извне. Ваш ввод оставлен, файл не перезаписан.')}else{acknowledge(path,latest.text);setDocument(result.document);setText(result.document.content);setExternal(null)}
    }catch(reason){if(mounted.current)setError(String(reason))}finally{writing.current=false;if(mounted.current)setBusy(false)}
  }
  const reload=async()=>{
    if(writing.current||dirty&&!window.confirm('Отбросить только текущий ввод связанного документа и загрузить версию с диска?'))return
    const previous=current.current.text;setBusy(true)
    try{const next=await invoke<NoteDocument>('read_linked_resource',{root,path});if(!mounted.current)return;acknowledge(path,previous);setDocument(next);setText(next.content);setExternal(null);setError('')}catch(reason){if(mounted.current)setError(String(reason))}finally{if(mounted.current)setBusy(false)}
  }
  const close=()=>{if(busy)return;if(dirty){setError('Есть несохранённый ввод. Сохраните его или явно отбросьте перед закрытием. Аварийная копия хранится отдельно от черновика основной заметки.');return}onClose()}
  const open=(target:string)=>{if(dirty){setError('Сначала сохраните или отбросьте текущую правку; переход не уничтожит её молча.');return}onOpen(target)}
  return <WorkspaceDialog title="Связанный документ / представление" onClose={close}><div className="linked-resource-dialog" onPointerDown={e=>e.stopPropagation()} onContextMenu={e=>e.stopPropagation()}><header><strong>{path}</strong><small>Источник в vault, не копия содержимого в Canvas</small></header>
    {readonly&&<p role="status">{available?'Карточка или её группа заблокирована.':'Ссылка удалена или изменена. Ваш ввод сохранён, запись из этого окна недоступна.'}</p>}{error&&<p role="alert">{error}</p>}
    {external&&<section className="linked-resource-conflict"><strong>{external==='unavailable'?'Источник недоступен':'На диске другая версия'}</strong><p>Автоматической перезаписи или подмены ввода нет. Загрузка версии с диска потребует явно отбросить несохранённый ввод.</p>{external!=='unavailable'&&<details><summary>Посмотреть версию на диске (первые 12000 символов)</summary><pre>{external.content.slice(0,12000)}</pre></details>}<button disabled={busy} onClick={()=>void reload()}>Загрузить с диска…</button></section>}
    {!document?<p>Читаю связанный источник…</p>:/\.base$/i.test(path)?<BaseView root={root} index={index} source={text} onChange={next=>change(next,true)} onOpen={open} onUpdateTask={onUpdateTask} busyTaskId={busyTaskId} locked={readonly||busy}/>:<><div className="linked-document-modes"><button aria-pressed={!edit} onClick={()=>setEdit(false)}>Читать</button><button aria-pressed={edit} onClick={()=>setEdit(true)}>Markdown</button></div>{edit?<textarea className="linked-document-editor" aria-label="Исходник связанного документа" spellCheck={false} disabled={readonly||busy} value={text} onChange={e=>change(e.target.value)}/>:<div className="linked-document-preview"><ReactMarkdown remarkPlugins={[remarkGfm]} components={{img:({alt})=><span className="linked-inert-media">[Вложение: {alt||'без подписи'}]</span>,a:({children,href})=><span className="linked-inert-link" title={href}>{children}</span>}}>{text.slice(0,32768)}</ReactMarkdown>{text.length>32768&&<p>Предпросмотр ограничен первыми 32768 символами. В режиме Markdown доступен полный исходник; сохранение не обрезает его.</p>}</div>}</>}
    <footer><button disabled={busy||dirty} onClick={()=>open(path)}>Открыть отдельно</button>{dirty&&<button disabled={busy} onClick={()=>{if(window.confirm('Отбросить только несохранённый ввод? Исходный файл останется без изменений.')){acknowledge(path,text);setText(document?.content??'');setError('')}}}>Отбросить ввод…</button>}<button disabled={busy} onClick={close}>Закрыть</button><button className="primary" disabled={busy||readonly||!dirty} onClick={()=>void save()}>{busy?'Проверяю источник…':'Сохранить источник'}</button></footer>
  </div></WorkspaceDialog>
}
