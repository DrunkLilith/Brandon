import {compileExpression,compareValues,type QueryRow} from './query-service.ts'
import type {TaskMetadata} from './types'
export type PlannerMode='list'|'kanban'|'calendar'|'board'
export type TaskFilter={text:string;status:'all'|'open'|'todo'|'doing'|'done';project:string;priority:string;tag:string;scheduled:'all'|'scheduled'|'unscheduled';expression:string}
export type TaskSort={property:'title'|'status'|'project'|'priority'|'scheduled'|'due'|'durationMinutes';direction:'ASC'|'DESC'}
export type TaskView={id:string;name:string;mode:PlannerMode;filter:TaskFilter;sort:TaskSort;raw:Record<string,unknown>;error:string|null}
export type TaskViewsDocument={value:{format:string;version:number;views:Record<string,unknown>[];[key:string]:unknown};fingerprint:string}
const fields=['id','title','status','project','priority','tags','completed','scheduled','due','durationMinutes','path','managed']
const collator=new Intl.Collator('ru',{numeric:true})
export function defaultTaskFilter():TaskFilter{return {text:'',status:'all',project:'',priority:'',tag:'',scheduled:'all',expression:''}}
export const defaultTaskSort:TaskSort={property:'scheduled',direction:'ASC'}
export function taskStatus(task:TaskMetadata){return task.status??(task.completed?'done':'todo')}
export function taskRow(task:TaskMetadata):QueryRow{return {field:name=>{const key=name.replace(/^task\./,'');if(!fields.includes(key))return null;if(key==='status')return taskStatus(task);return task[key as keyof TaskMetadata]},hasTag:tag=>task.tags.some(value=>value===tag||value.startsWith(tag+'/')),inFolder:folder=>task.path.startsWith(folder.replace(/\/$/,'')+'/'),hasLink:()=>false}}
export function queryTasks(tasks:TaskMetadata[],filter:TaskFilter,sort:TaskSort):TaskMetadata[]{
  const expression=filter.expression.trim()?compileExpression(filter.expression,name=>{if(!fields.includes(name.replace(/^task\./,'')))throw new Error(`Недоступное поле задачи: ${name}`)},name=>{if(name==='file.hasLink')throw new Error('hasLink недоступен в task query: используйте запрос к заметкам')}):()=>true
  const text=filter.text.trim().toLocaleLowerCase('ru'),tag=filter.tag.replace(/^#/,'')
  const result=tasks.filter(task=>(!text||`${task.title} ${task.project??''} ${task.path} ${task.tags.join(' ')}`.toLocaleLowerCase('ru').includes(text))&&(filter.status==='all'||filter.status==='open'&&!task.completed||taskStatus(task)===filter.status)&&(!filter.project||task.project===filter.project)&&(!filter.priority||(filter.priority==='normal'?task.priority===null:task.priority===filter.priority))&&(!tag||taskRow(task).hasTag(tag))&&(filter.scheduled==='all'||filter.scheduled==='scheduled'&&Boolean(task.scheduled)||filter.scheduled==='unscheduled'&&!task.scheduled)&&Boolean(expression(taskRow(task))))
  const priority:Record<string,number>={urgent:0,high:1,normal:2,low:3,lowest:4}
  return result.sort((a,b)=>{const left=sort.property==='priority'?priority[a.priority??'normal']:taskRow(a).field(sort.property),right=sort.property==='priority'?priority[b.priority??'normal']:taskRow(b).field(sort.property);if(left==null&&right!=null)return 1;if(right==null&&left!=null)return -1;return compareValues(left,right,collator)*(sort.direction==='DESC'?-1:1)||collator.compare(a.id,b.id)})
}
export function parseTaskView(raw:Record<string,unknown>):TaskView{
  const id=typeof raw.id==='string'?raw.id:'',name=typeof raw.name==='string'?raw.name:'Без названия',filter=defaultTaskFilter();let mode:PlannerMode='calendar',sort={...defaultTaskSort}
  try{
    if(!id||!['list','kanban','calendar','board'].includes(String(raw.mode)))throw new Error('Неизвестный режим task view');mode=raw.mode as PlannerMode
    if(raw.filter!==undefined){if(!raw.filter||typeof raw.filter!=='object'||Array.isArray(raw.filter))throw new Error('Некорректный фильтр');const candidate=raw.filter as Record<string,unknown>;for(const key of Object.keys(candidate)){if(!Object.hasOwn(filter,key))throw new Error(`Неизвестный фильтр «${key}» не применён`);if(typeof candidate[key]!=='string'||(candidate[key] as string).length>4096)throw new Error(`Некорректное поле фильтра ${key}`)}Object.assign(filter,candidate)}
    if(!['all','open','todo','doing','done'].includes(filter.status)||!['all','scheduled','unscheduled'].includes(filter.scheduled)||!['','normal','urgent','high','low','lowest'].includes(filter.priority))throw new Error('Неподдерживаемое значение фильтра')
    if(raw.sort!==undefined){if(!raw.sort||typeof raw.sort!=='object'||Array.isArray(raw.sort))throw new Error('Некорректная сортировка');const value=raw.sort as Record<string,unknown>;if(!['title','status','project','priority','scheduled','due','durationMinutes'].includes(String(value.property))||!['ASC','DESC'].includes(String(value.direction))||Object.keys(value).some(key=>!['property','direction'].includes(key)))throw new Error('Неподдерживаемая сортировка');sort={property:value.property as TaskSort['property'],direction:value.direction as TaskSort['direction']}}
    queryTasks([],filter,sort);return {id,name,mode,filter,sort,raw,error:null}
  }catch(error){return {id,name,mode,filter,sort,raw,error:String(error)}}
}
