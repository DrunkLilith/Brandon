import {useState} from 'react'
import {WorkspaceDialog} from './workspace-dialogs'
export function CanvasStickyStack({onApply,onClose}:{onApply:(lines:string[])=>void;onClose:()=>void}){
  const [text,setText]=useState(''),[error,setError]=useState(''),lines=text.split(/\r?\n/).filter(line=>line.trim())
  return <WorkspaceDialog title="Стикеры списком" onClose={onClose}><div className="canvas-style-dialog"><p>Каждая непустая строка станет отдельным стикером. До 100 штук за один Undo.</p><textarea aria-label="Строки для стикеров" maxLength={65536} value={text} onChange={event=>setText(event.target.value)}/><small>{lines.length} стикеров</small>{error&&<p role="alert">{error}</p>}<footer><button onClick={onClose}>Отмена</button><button className="primary" disabled={!lines.length||lines.length>100} onClick={()=>{try{if(!lines.length||lines.length>100)throw Error('Допустимо от 1 до 100 строк');onApply(lines);onClose()}catch(reason){setError(String(reason))}}}>Создать стикеры</button></footer></div></WorkspaceDialog>
}
