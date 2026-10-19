import type {CanvasNode} from './canvas-model'
import {canvasExtension} from './canvas-model.ts'
import type {TaskMetadata} from './types'

export const canvasTaskId=(node:CanvasNode|undefined)=>node?.type==='file'?canvasExtension(node)?.taskId:undefined
export const taskStatusLabel=(status:TaskMetadata['status'])=>({todo:'К выполнению',doing:'В работе',done:'Готово'}[status])
const samePath=(a:string,b:string)=>a.toLocaleLowerCase()===b.toLocaleLowerCase()
export const validTaskFallback=(path:unknown):path is string=>typeof path==='string'&&path.length<=4096&&path.toLowerCase().endsWith('.md')&&!/[\\:\x00-\x1f\x7f]/.test(path)&&!path.split('/').some(part=>!part||part==='.'||part==='..'||part.startsWith('.'))
export function canvasTaskBinding(node:CanvasNode,tasks:ReadonlyMap<string,TaskMetadata>,ready=true){
  const id=canvasTaskId(node);if(!id)return null
  const task=tasks.get(id)
  const error=!ready?'Метаданные задач ещё загружаются':!task?'Задача недоступна':!task.managed||!task.revision?'Это не каноническая задача':!samePath(node.file??'',task.path)?'Путь описания изменился':null
  return{id,task,error}
}
export function canvasTaskLabel(node:CanvasNode,tasks:ReadonlyMap<string,TaskMetadata>,ready=true){
  const binding=canvasTaskBinding(node,tasks,ready);if(!binding)return undefined
  if(binding.error)return `${binding.error}\n${node.file??''}`
  const task=binding.task!
  return `${task.completed?'☑':'☐'} ${task.title}\n${taskStatusLabel(task.status)}${task.project?' · '+task.project:''}\n${task.scheduled?'План: '+task.scheduled+'  ':''}${task.due?'Срок: '+task.due:''}`.trimEnd()
}
export function taskReferenceNode(id:string,task:TaskMetadata,center:{x:number;y:number}):CanvasNode{
  if(!task.managed||!task.revision||!/^[A-Za-z0-9_-]{1,128}$/.test(task.id)||!validTaskFallback(task.path))throw Error('Добавляйте существующую каноническую задачу')
  return{id,type:'file',file:task.path,x:center.x-170,y:center.y-90,width:340,height:180,brandon:{version:1,taskId:task.id}}
}
