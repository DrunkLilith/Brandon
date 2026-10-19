import {useEffect,useRef,useState} from 'react'
import {invoke} from '@tauri-apps/api/core'
import {open} from '@tauri-apps/plugin-dialog'
import {WorkspaceDialog} from './workspace-dialogs'
import {canvasExportScene,canvasExportWarnings,type CanvasExportReport} from './canvas-export'
import type {CanvasNode} from './canvas-model'

type Prepared={requestId:string;report:CanvasExportReport}
type Saved={directory:string;pngPath:string;reportPath:string}
export function CanvasExportDialog({root,sourcePath,source,currentSource,frames,initialFrame,onClose}:{root:string;sourcePath:string;source:string;currentSource:string;frames:CanvasNode[];initialFrame:string;onClose:()=>void}){
  const [frameId,setFrameId]=useState(initialFrame),[scale,setScale]=useState<1|2>(1),[transparent,setTransparent]=useState(false)
  const [prepared,setPrepared]=useState<Prepared|null>(null),[preview,setPreview]=useState(''),[saved,setSaved]=useState<Saved|null>(null),[error,setError]=useState(''),[busy,setBusy]=useState<'prepare'|'write'|null>(null)
  const job=useRef<string|null>(null),generation=useRef(0),mounted=useRef(false),url=useRef(''),latestSource=useRef(currentSource);latestSource.current=currentSource
  const stale=currentSource!==source
  const release=()=>{const id=job.current;job.current=null;if(id)void invoke('canvas_export_cancel',{root,requestId:id}).catch(()=>{});if(url.current){URL.revokeObjectURL(url.current);url.current=''}}
  useEffect(()=>{mounted.current=true;return()=>{mounted.current=false;generation.current++;release()}},[root]) // A request belongs to this mounted vault snapshot only.
  useEffect(()=>{if(currentSource!==source)release()},[currentSource,source])
  const reset=()=>{generation.current++;release();setPrepared(null);setPreview('');setSaved(null);setError('')}
  const prepare=async()=>{
    reset();const seq=generation.current;setBusy('prepare')
    try{
      const scene=canvasExportScene(source,frameId,scale,transparent),requestId=crypto.randomUUID();job.current=requestId
      const next=await invoke<Prepared>('canvas_export_prepare',{request:{root,requestId,sourcePath,source,frameId,scene}})
      if(!mounted.current||seq!==generation.current)return
      const bytes=await invoke<ArrayBuffer>('canvas_export_preview',{root,requestId})
      if(!mounted.current||seq!==generation.current)return
      url.current=URL.createObjectURL(new Blob([bytes],{type:'image/png'}));setPreview(url.current);setPrepared(next)
    }catch(reason){if(mounted.current&&seq===generation.current){release();setError(String(reason))}}
    finally{if(mounted.current&&seq===generation.current)setBusy(null)}
  }
  const write=async()=>{
    if(!prepared||stale||busy)return
    const seq=generation.current;setBusy('write');setError('')
    try{
      const folder=await open({directory:true,multiple:false,title:'Папка PNG-экспорта — вне исходного vault'})
      if(!mounted.current||seq!==generation.current||latestSource.current!==source||typeof folder!=='string')return
      const result=await invoke<Saved>('canvas_export_write',{root,requestId:prepared.requestId,folder})
      if(mounted.current){setSaved(result);setBusy(null)}
    }catch(reason){if(mounted.current&&seq===generation.current)setError(String(reason))}
    finally{if(mounted.current&&seq===generation.current)setBusy(null)}
  }
  const close=()=>{if(busy!=='write')onClose()}
  const warnings=prepared?canvasExportWarnings(prepared.report):[]
  return <WorkspaceDialog title="Экспорт фрейма в PNG" onClose={close}><div className="canvas-export-dialog" onPointerDown={e=>e.stopPropagation()} onContextMenu={e=>e.stopPropagation()}>
    <p>Отдельное изображение выбранной сцены и отчёт о преобразовании. Доска, вложения и заметки докладчика не копируются и не изменяются. Перед подготовкой сохраните доску.</p>
    <div className="canvas-export-options"><label>Фрейм<select aria-label="Фрейм для PNG" disabled={Boolean(busy)||Boolean(saved)||stale} value={frameId} onChange={e=>{reset();setFrameId(e.target.value)}}>{frames.map(n=><option key={n.id} value={n.id}>{n.label||'Без названия'} · {Math.round(n.width)} × {Math.round(n.height)}</option>)}</select></label>
    <label>Масштаб<select aria-label="Масштаб PNG" disabled={Boolean(busy)||Boolean(saved)||stale} value={scale} onChange={e=>{reset();setScale(Number(e.target.value) as 1|2)}}><option value="1">1×</option><option value="2">2×</option></select></label>
    <label className="canvas-export-checkbox"><input type="checkbox" checked={transparent} disabled={Boolean(busy)||Boolean(saved)||stale} onChange={e=>{reset();setTransparent(e.target.checked)}}/>Прозрачный фон</label></div>
    {stale&&!saved&&<p role="alert">Доска изменилась. Закройте экспорт, сохраните изменения и откройте его заново.</p>}
    {error&&<p role="alert">{error}</p>}
    {busy==='prepare'&&<p role="status">Готовлю изображение и проверяю доступность источников…</p>}
    {preview&&<figure className="canvas-export-preview"><img src={preview} alt="Предпросмотр экспортируемого фрейма"/><figcaption>Предпросмотр уменьшен для окна. Полный PNG: {prepared?.report.render.width} × {prepared?.report.render.height} px.</figcaption></figure>}
    {prepared&&<section className="canvas-export-report" aria-label="Проверка PNG"><strong>{prepared.report.frameLabel}</strong><p>{prepared.report.render.nodes} объектов · {prepared.report.render.edges} связей · {prepared.report.render.inkStrokes} штрихов</p><p>Шрифты: {prepared.report.render.fontFamilies.join(', ')||'Текст отсутствует'}. В PNG это пиксели, не встроенные файлы шрифтов.</p>
      {warnings.length>0&&<ul>{warnings.map(w=><li key={w}>{w}</li>)}</ul>}
      <small>PNG — плоская копия: без редактирования, слоёв, живых ссылок и обновления задач. Текст — подготовленный обычный текст; оформление Markdown может отличаться от карточки. Веб-страницы, PDF и другие нерастровые файлы остаются подписями. Проверьте предпросмотр. Подготовленный результат действует 3 минуты.</small>
    </section>}
    {saved&&<section className="canvas-export-saved" role="status"><strong>Экспорт сохранён</strong><p>{saved.pngPath}</p><p>{saved.reportPath}</p><small>Оба файла находятся в новой отдельной папке. Повторное нажатие не создаёт дубликат.</small></section>}
    <footer><button disabled={busy==='write'} onClick={close}>{busy==='prepare'?'Отменить и закрыть':saved?'Готово':'Закрыть'}</button>{!saved&&<><button disabled={Boolean(busy)||stale} onClick={()=>void prepare()}>{prepared?'Подготовить заново':'Подготовить предпросмотр'}</button><button className="primary" disabled={!prepared||Boolean(busy)||stale} onClick={()=>void write()}>{busy==='write'?'Сохраняю…':'Выбрать папку и сохранить'}</button></>}</footer>
  </div></WorkspaceDialog>
}
