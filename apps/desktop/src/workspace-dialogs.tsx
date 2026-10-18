import { useEffect, useRef, useState, type ReactNode } from 'react'
import { invoke } from '@tauri-apps/api/core'
import type { CrashDraft } from './workspace-state'

export function WorkspaceDialog({title,onClose,children}: {title:string;onClose:()=>void;children:ReactNode}) {
  const ref=useRef<HTMLDivElement>(null)
  useEffect(()=>{const previous=document.activeElement as HTMLElement|null;ref.current?.querySelector<HTMLButtonElement>('button')?.focus();return()=>{if(previous?.isConnected)previous.focus()}},[])
  return <div className="dialog-backdrop workspace-dialog-backdrop"><div ref={ref} className="workspace-dialog" role="dialog" aria-modal="true" aria-label={title} onKeyDown={event=>{
    event.stopPropagation()
    if(event.key==='Escape'){event.preventDefault();onClose()}
    if(event.key==='Tab'){const items=Array.from(ref.current?.querySelectorAll<HTMLElement>('button:not(:disabled),input,textarea,select,[tabindex="0"]')??[]);const index=items.indexOf(document.activeElement as HTMLElement);event.preventDefault();items[(index+(event.shiftKey?-1:1)+items.length)%items.length]?.focus()}
  }}><header><h2>{title}</h2><button onClick={onClose} aria-label={`Закрыть: ${title}`}>×</button></header>{children}</div></div>
}

export function TextComparison({left,right,leftLabel='На диске',rightLabel='Черновик'}:{left:string;right:string;leftLabel?:string;rightLabel?:string}) {
  const a=left.split('\n'),b=right.split('\n'),length=Math.max(a.length,b.length)
  return <div className="text-comparison"><section><h3>{leftLabel}</h3><pre>{Array.from({length},(_,i)=><div className={a[i]!==b[i]?'diff-removed':''} key={i}><span>{i+1}</span>{a[i]??' '}</div>)}</pre></section><section><h3>{rightLabel}</h3><pre>{Array.from({length},(_,i)=><div className={a[i]!==b[i]?'diff-added':''} key={i}><span>{i+1}</span>{b[i]??' '}</div>)}</pre></section></div>
}

export function RecoveryCenter({drafts,onRestore,onDiscard,onClose}:{drafts:CrashDraft[];onRestore:(draft:CrashDraft)=>void;onDiscard:(draft:CrashDraft)=>void;onClose:()=>void}) {
  const [selected,setSelected]=useState(drafts[0]?.version)
  const current=drafts.find(d=>d.version===selected)??drafts[0]
  return <WorkspaceDialog title="Восстановление черновиков" onClose={onClose}><p>Найдены несохранённые версии после предыдущей сессии. Открытие черновика не перезаписывает оригинал. Если файл изменился, потребуется разрешить конфликт или сохранить копию.</p><div className="recovery-list">{drafts.map(d=><button key={d.version} className={d.version===current?.version?'active':''} onClick={()=>setSelected(d.version)}><strong>{d.path}</strong><small>{d.pane==='main'?'Основная панель':'Вторая панель'} · {new Date(d.updated).toLocaleString('ru')}</small></button>)}</div>{current?<><TextComparison left={current.baseContent} right={current.content} leftLabel="Исходная версия"/><footer><button onClick={()=>{if(window.confirm('Убрать только эту аварийную копию? Файл на диске не изменится.'))onDiscard(current)}}>Отбросить копию…</button><button className="primary" onClick={()=>onRestore(current)}>Восстановить в редактор</button></footer></>:<p>Невосстановленных черновиков нет.</p>}</WorkspaceDialog>
}

export function VersionHistory({root,path,onRestore,onClose}:{root:string;path:string;onRestore:(text:string)=>void;onClose:()=>void}) {
  const [items,setItems]=useState<{id:number;updated:number;content:string}[]>([]),[error,setError]=useState(''),[selected,setSelected]=useState<number|null>(null)
  useEffect(()=>{let alive=true;void invoke<typeof items>('document_versions',{root,path}).then(values=>{if(alive){setItems(values);setSelected(values[0]?.id??null)}}).catch(reason=>{if(alive)setError(String(reason))});return()=>{alive=false}},[root,path])
  const current=items.find(item=>item.id===selected)
  return <WorkspaceDialog title="История документа" onClose={onClose}><p>{path} · до 20 сохранённых исходных версий на этом устройстве. Это не замена резервной копии vault.</p>{error&&<p role="alert">{error}</p>}<div className="recovery-list">{items.map(item=><button key={item.id} className={item.id===selected?'active':''} onClick={()=>setSelected(item.id)}>{new Date(item.updated).toLocaleString('ru')}</button>)}</div>{current?<><pre className="version-source">{current.content}</pre><footer><button onClick={()=>onRestore(current.content)}>Открыть эту версию как правку</button></footer></>:<p>История появится после редактирования документа в этой версии Brandon.</p>}</WorkspaceDialog>
}
