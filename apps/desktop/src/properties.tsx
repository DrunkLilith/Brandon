import { invoke } from '@tauri-apps/api/core'
import { useEffect, useRef, useState } from 'react'
export type PropertyKind='text'|'list'|'number'|'checkbox'|'date'|'datetime'|'tags'
export type PropertyPatch={key:string;kind:PropertyKind;value:unknown;remove:boolean}
export type PropertyInspection={values:Record<string,unknown>;editable:Record<string,boolean>;error:string|null}
export function frontmatterRange(source:string):{start:number;end:number}|null {
  const match=/^\uFEFF?---\r?\n/.exec(source);if(!match)return null
  const closing=/^---(?:\r?\n|$)/gm;closing.lastIndex=match[0].length;const end=closing.exec(source)
  return end?{start:match[0].length,end:end.index+end[0].length}:null
}
export function withoutFrontmatter(source:string){const range=frontmatterRange(source);return range?source.slice(0,range.end).replace(/[^\r\n]/g,'')+source.slice(range.end):source}
export function displayProperty(value:unknown){if(value==null)return '—';if(Array.isArray(value))return value.map(item=>item!==null&&typeof item==='object'?JSON.stringify(item):String(item)).join(', ');if(typeof value==='object')return JSON.stringify(value);if(typeof value==='boolean')return value?'Да':'Нет';return String(value)}
export function inferProperty(key:string,value:unknown):PropertyKind {if(key==='tags')return 'tags';if(Array.isArray(value))return 'list';if(typeof value==='number')return 'number';if(typeof value==='boolean')return 'checkbox';if(typeof value==='string'&&/^\d{4}-\d{2}-\d{2}$/.test(value))return 'date';if(typeof value==='string'&&/^\d{4}-\d{2}-\d{2}T/.test(value))return 'datetime';return 'text'}
const kinds:{value:PropertyKind;label:string}[]=[{value:'text',label:'Текст'},{value:'list',label:'Список'},{value:'number',label:'Число'},{value:'checkbox',label:'Флажок'},{value:'date',label:'Дата'},{value:'datetime',label:'Дата и время'},{value:'tags',label:'Теги'}]
export function PropertyDialog({name,value,onApply,onClose}:{name:string;value:unknown;onApply:(patch:PropertyPatch)=>Promise<void>;onClose:()=>void}){
  const [key,setKey]=useState(name),[kind,setKind]=useState<PropertyKind>(inferProperty(name,value)),[text,setText]=useState(Array.isArray(value)?JSON.stringify(value):value==null?'':String(value)),[checked,setChecked]=useState(value===true),[error,setError]=useState(''),[busy,setBusy]=useState(false)
  const ref=useRef<HTMLFormElement>(null)
  useEffect(()=>{const previous=document.activeElement as HTMLElement|null;ref.current?.querySelector<HTMLInputElement>(name?'[aria-label="Значение свойства"]':'input:not(:disabled)')?.focus();return()=>{if(previous?.isConnected)previous.focus()}},[name])
  const submit=async(remove=false)=>{setError('');setBusy(true);try{
    let next:unknown=text
    if(kind==='number'&&!remove){if(!text.trim()||!Number.isFinite(Number(text)))throw new Error('Введите конечное число');next=Number(text)}
    if(kind==='checkbox')next=checked
    if((kind==='list'||kind==='tags')&&!remove){try{next=JSON.parse(text)}catch{throw new Error('Список задаётся как JSON: ["первый", "второй"]')}}
    await onApply({key,kind,value:next,remove});onClose()
  }catch(error){setError(String(error))}finally{setBusy(false)}}
  return <div className="dialog-backdrop"><form ref={ref} className="dialog property-dialog" role="dialog" aria-modal="true" aria-label="Редактирование свойства" onSubmit={event=>{event.preventDefault();void submit()}} onKeyDown={event=>{event.stopPropagation();if(event.key==='Escape'&&!busy)onClose();if(event.key==='Tab'){const controls=Array.from(ref.current?.querySelectorAll<HTMLElement>('button:not(:disabled),input:not(:disabled),select,textarea')??[]);if(event.shiftKey&&document.activeElement===controls[0]){event.preventDefault();controls.at(-1)?.focus()}else if(!event.shiftKey&&document.activeElement===controls.at(-1)){event.preventDefault();controls[0]?.focus()}}}}><h2>{name?'Свойство':'Добавить свойство'}</h2>
    <label>Имя<input aria-label="Имя свойства" value={key} disabled={Boolean(name)||busy} onChange={event=>setKey(event.target.value)}/></label>
    <label>Тип<select aria-label="Тип свойства" value={kind} disabled={busy} onChange={event=>setKind(event.target.value as PropertyKind)}>{kinds.map(item=><option key={item.value} value={item.value}>{item.label}</option>)}</select></label>
    {kind==='checkbox'?<label>Значение<input aria-label="Значение свойства" type="checkbox" checked={checked} disabled={busy} onChange={event=>setChecked(event.target.checked)}/></label>:<label>Значение{kind==='list'||kind==='tags'?<textarea aria-label="Значение свойства" rows={4} value={text} disabled={busy} onChange={event=>setText(event.target.value)} placeholder={'["первый", "второй"]'}/>:<input aria-label="Значение свойства" type={kind==='date'?'date':kind==='number'?'number':'text'} step="any" value={text} disabled={busy} onChange={event=>setText(event.target.value)} placeholder={kind==='datetime'?'2026-10-19T12:30':''}/>}</label>}
    <p>Меняется только это поле. Остальные свойства и текст остаются на месте.</p>{error&&<p className="property-error" role="alert">{error}</p>}<div>{name&&<button type="button" className="property-remove" disabled={busy} onClick={()=>void submit(true)}>Удалить свойство</button>}<button type="button" disabled={busy} onClick={onClose}>Отмена</button><button className="primary property-apply" disabled={busy||!key.trim()}>{busy?'Сохраняю…':'Применить'}</button></div>
  </form></div>
}
export function PropertiesEditor({source,onChange,mode='visible'}:{source:string;onChange:(value:string)=>void;mode?:'visible'|'hidden'|'source'}){
  const [inspection,setInspection]=useState<PropertyInspection|null>(null),[target,setTarget]=useState<{key:string;value:unknown;source:string}|null>(null)
  const current=useRef(source);current.current=source
  const alive=useRef(true);useEffect(()=>{alive.current=true;return()=>{alive.current=false}},[])
  useEffect(()=>{let cancelled=false;const timer=setTimeout(()=>{void invoke<PropertyInspection>('inspect_note_properties',{contents:source}).then(value=>{if(!cancelled)setInspection(value)}).catch(error=>{if(!cancelled)setInspection({values:{},editable:{},error:String(error)})})},120);return()=>{cancelled=true;clearTimeout(timer)}},[source])
  if(mode==='hidden')return null
  return <section className="note-properties" aria-label="Свойства заметки"><header><strong>Свойства</strong><span>{mode==='source'?'YAML отображается в исходном редакторе':''}</span>{mode==='visible'&&!inspection?.error&&<button className="property-add" onClick={()=>setTarget({key:'',value:'',source})}>+ Свойство</button>}</header>
    {inspection?.error?<p className="property-error" role="alert">{inspection.error}</p>:mode==='visible'&&<div className="property-fields">{Object.entries(inspection?.values??{}).map(([key,value])=><button className="property-field" key={key} disabled={!inspection?.editable[key]} title={inspection?.editable[key]?'Изменить свойство':'Вложенное или сложное значение: откройте исходник YAML'} onClick={()=>setTarget({key,value,source})}><span>{key}</span><code>{displayProperty(value)}</code></button>)}{!Object.keys(inspection?.values??{}).length&&<small>Добавьте дату, статус, теги или другие поля.</small>}</div>}
    {target&&<PropertyDialog name={target.key} value={target.value} onClose={()=>setTarget(null)} onApply={async patch=>{if(current.current!==target.source)throw new Error('Текст изменился, пока открыто свойство. Закройте окно и откройте заново.');const next=await invoke<string>('patch_properties_source',{contents:target.source,patch});if(!alive.current)return;if(current.current!==target.source)throw new Error('Текст изменился. Повторите правку на новой версии.');onChange(next)}}/>}
  </section>
}
