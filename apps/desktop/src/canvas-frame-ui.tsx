import {useState} from 'react'
import {WorkspaceDialog} from './workspace-dialogs'
import type {CanvasFramePreset,CanvasNode} from './canvas-model'
import {CANVAS_FRAME_PRESETS,canvasFrame,FRAME_NOTES_LIMIT} from './canvas-frames'

export function CanvasFrameDialog({node,wrap,onApply,onClose}:{node?:CanvasNode;wrap:boolean;onApply:(value:{label:string;notes:string;preset:CanvasFramePreset})=>void;onClose:()=>void}){
  const [label,setLabel]=useState(node?.label??'Новый фрейм'),[notes,setNotes]=useState(canvasFrame(node)?.notes??''),[preset,setPreset]=useState<CanvasFramePreset>(canvasFrame(node)?.preset??'16:9'),[error,setError]=useState('')
  return <WorkspaceDialog title={node?'Фрейм и заметки':'Создать фрейм'} onClose={onClose}><div className="canvas-frame-dialog">
    <label>Название<input autoFocus aria-label="Название фрейма" maxLength={512} value={label} onChange={e=>setLabel(e.target.value)}/></label>
    {!node&&<label>Начальный размер<select aria-label="Размер фрейма" value={preset} onChange={e=>setPreset(e.target.value as CanvasFramePreset)}>{CANVAS_FRAME_PRESETS.map(p=><option value={p.id} key={p.id}>{p.label} · {p.width} × {p.height}</option>)}</select></label>}
    {!node&&<p>{wrap?'Фрейм охватит выделение с отступом. Содержимое не перемещается и не копируется.':'Пустой фрейм появится в центре текущего вида.'} Соотношение сторон не блокирует последующее изменение размера.</p>}
    <label>Заметки докладчика<textarea aria-label="Заметки фрейма" maxLength={FRAME_NOTES_LIMIT} rows={6} value={notes} onChange={e=>setNotes(e.target.value)}/></label>
    <small>Обычная группа Canvas с расширением Brandon. Порядок показа и заметки могут не поддерживаться другим редактором.</small>
    {error&&<p role="alert">{error}</p>}<footer><button onClick={onClose}>Отмена</button><button className="primary" onClick={()=>{try{onApply({label,notes,preset});onClose()}catch(reason){setError(reason instanceof Error?reason.message:String(reason))}}}>{node?'Сохранить фрейм':'Создать фрейм'}</button></footer>
  </div></WorkspaceDialog>
}
export function CanvasFramePanel({frames,selected,editable,onFocus,onEdit,onMove,onRemove,onCreate,onPresent,onClose}:{frames:CanvasNode[];selected:Set<string>;editable:(id:string)=>boolean;onFocus:(id:string)=>void;onEdit:(id:string)=>void;onMove:(id:string,direction:-1|1)=>void;onRemove:(id:string)=>void;onCreate:()=>void;onPresent:()=>void;onClose:()=>void}){
  return <aside className="canvas-frame-panel" aria-label="Фреймы доски"><header><div><strong>Фреймы</strong><small>{frames.length} · порядок показа</small></div><button onClick={onClose} aria-label="Закрыть фреймы">×</button></header><div className="canvas-frame-panel-actions"><button onClick={onCreate}>+ Фрейм</button><button className="primary" disabled={!frames.length} onClick={onPresent}>Показать</button></div>
    <ol>{frames.map((node,i)=><li key={node.id} data-frame-id={node.id} className={selected.has(node.id)?'active':''}><button className="canvas-frame-target" onClick={()=>onFocus(node.id)}><span>{i+1}</span><strong>{node.label||'Без названия'}</strong><small>{Math.round(node.width)} × {Math.round(node.height)}</small></button><div><button aria-label={`Изменить фрейм ${i+1}`} disabled={!editable(node.id)} onClick={()=>onEdit(node.id)}>Изменить</button><button aria-label={`Поднять фрейм ${i+1}`} disabled={!i||!editable(node.id)} onClick={()=>onMove(node.id,-1)}>↑</button><button aria-label={`Опустить фрейм ${i+1}`} disabled={i===frames.length-1||!editable(node.id)} onClick={()=>onMove(node.id,1)}>↓</button><button aria-label={`Снять рамку ${i+1}`} title="Убрать только фрейм, сохранить содержимое" disabled={!editable(node.id)} onClick={()=>onRemove(node.id)}>Снять рамку</button></div></li>)}</ol>
    {!frames.length&&<p>Разделите доску на сцены. Выделите объекты и создайте фрейм вокруг них или начните с пустого.</p>}<small>Перестановка слайдов не меняет слои объектов.</small>
  </aside>
}
export function CanvasPresentationControls({frames,id,notes,onNavigate,onExit}:{frames:CanvasNode[];id:string;notes:boolean;onNavigate:(id:string)=>void;onExit:()=>void}){
  const [showNotes,setShowNotes]=useState(notes),index=frames.findIndex(n=>n.id===id),node=frames[index]
  if(!node)return null
  return <><nav className="canvas-presentation-controls" aria-label="Показ фреймов"><button aria-label="Предыдущий фрейм" disabled={index===0} onClick={()=>onNavigate(frames[index-1].id)}>←</button><span><strong>{node.label||'Фрейм'}</strong><small>{index+1} / {frames.length}</small></span><button aria-label="Следующий фрейм" disabled={index===frames.length-1} onClick={()=>onNavigate(frames[index+1].id)}>→</button><button aria-pressed={showNotes} onClick={()=>setShowNotes(v=>!v)}>Заметки</button><button onClick={onExit}>Завершить · Esc</button></nav>{showNotes&&<aside className="canvas-speaker-notes" aria-label="Заметки докладчика"><strong>{node.label}</strong><p>{canvasFrame(node)?.notes||'У этого фрейма пока нет заметок.'}</p></aside>}</>
}
