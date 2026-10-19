import {CANVAS_PRIMITIVES} from './canvas-primitives.ts'
export const CANVAS_CREATE_COMMANDS=[...CANVAS_PRIMITIVES.map(primitive=>({id:`canvas-add-${primitive.id}`,label:`Добавить: ${primitive.label}`})),{id:'canvas-add-stack',label:'Стикеры списком'},{id:'canvas-add-task',label:'Добавить задачу'}]
export const CANVAS_INK_COMMANDS=[{id:'canvas-tool-pen',label:'Перо',defaultShortcut:'Alt+KeyP'},{id:'canvas-tool-highlighter',label:'Маркер',defaultShortcut:'Alt+KeyH'},{id:'canvas-tool-eraser',label:'Ластик штрихов',defaultShortcut:'Alt+KeyE'}]as const
export const CANVAS_FRAME_COMMANDS=[{id:'canvas-frames',label:'Фреймы доски'},{id:'canvas-add-frame',label:'Создать фрейм'},{id:'canvas-frame-details',label:'Название и заметки фрейма'},{id:'canvas-frame-remove',label:'Снять только рамку'},{id:'canvas-export-png',label:'Экспорт фрейма в PNG'},{id:'canvas-present',label:'Показать фреймы',defaultShortcut:'Mod+Alt+KeyP'},{id:'canvas-present-end',label:'Завершить показ'},{id:'canvas-present-prev',label:'Предыдущий фрейм'},{id:'canvas-present-next',label:'Следующий фрейм'}]as const
// Descriptors are shared by the app palette, board toolbar/context and keyboard.
export const CANVAS_COMMANDS=[
  {id:'canvas-group',label:'Сгруппировать',defaultShortcut:'Mod+Shift+KeyG'},
  {id:'canvas-ungroup',label:'Разгруппировать',defaultShortcut:'Mod+Alt+KeyG'},
  {id:'canvas-lock',label:'Заблокировать / разблокировать',defaultShortcut:'Mod+Shift+KeyL'},
  {id:'canvas-front',label:'На передний план'},
  {id:'canvas-back',label:'На задний план'},
  {id:'canvas-left',label:'Выровнять по левому краю'},
  {id:'canvas-center-x',label:'Выровнять по центру по горизонтали'},
  {id:'canvas-right',label:'Выровнять по правому краю'},
  {id:'canvas-top',label:'Выровнять по верхнему краю'},
  {id:'canvas-center-y',label:'Выровнять по центру по вертикали'},
  {id:'canvas-bottom',label:'Выровнять по нижнему краю'},
  {id:'canvas-distribute-x',label:'Равные промежутки по горизонтали'},
  {id:'canvas-distribute-y',label:'Равные промежутки по вертикали'},
  {id:'canvas-stack',label:'Разложить стопкой'},
  {id:'canvas-grid',label:'Разложить сеткой'},
  {id:'canvas-style',label:'Стиль выбранных объектов'},
  {id:'canvas-task-details',label:'Поля выбранной задачи'},
  {id:'canvas-linked-resource',label:'Открыть связанный Document / Table / Kanban / Timeline'},
  {id:'canvas-edge-style',label:'Стиль выбранной связи'},
  {id:'canvas-edge-reverse',label:'Развернуть выбранную связь'},
  {id:'canvas-edge-delete',label:'Удалить выбранную связь'},
  ...CANVAS_CREATE_COMMANDS,
  ...CANVAS_INK_COMMANDS,
  ...CANVAS_FRAME_COMMANDS,
] as const

export function dispatchCanvasCommand(element:Element|null,id:string){
  if(!element)return false
  const detail={id,handled:false}
  element.dispatchEvent(new CustomEvent('brandon-canvas-command',{detail}))
  return detail.handled
}
