import {defaultTaskFilter,parseTaskView,type TaskFilter,type TaskSort} from './task-query.ts'
import type {TaskMetadata,TaskPatch} from './types'

export type TaskFormatLayout='table'|'kanban'|'timeline'
export type TaskFormatView={id:string;name:string;layout:TaskFormatLayout;sort:TaskSort;windowDays:7|14|30|60}
export type TaskFormatDefinition={filter:TaskFilter;views:TaskFormatView[];warnings:string[];raw:Record<string,unknown>}
const object=(v:unknown):v is Record<string,unknown>=>Boolean(v)&&typeof v==='object'&&!Array.isArray(v)
export const isWorkingFormat=(v:unknown)=>object(v)&&Object.hasOwn(v,'brandon')
export function compileTaskFormat(value:unknown):TaskFormatDefinition{
  if(!object(value)||!object(value.brandon)||value.brandon.version!==1||value.brandon.format!=='task-view')throw Error('Неизвестный формат или версия Brandon; исходник не изменён')
  if(!Array.isArray(value.views)||!value.views.length||value.views.length>16)throw Error('От 1 до 16 связанных представлений')
  const warnings:string[]=[],ids=new Set<string>();let ignoredCount=0
  const ignored=(record:Record<string,unknown>,known:string[],path:string)=>{for(const key of Object.keys(record))if(!known.includes(key)){ignoredCount++;if(warnings.length<128)warnings.push(`${path}${key.slice(0,200)}: сохранено, но не исполняется`)}}
  ignored(value,['brandon','views'],'');ignored(value.brandon,['version','format','filter'],'brandon.')
  const filter=parseTaskView({id:'definition',mode:'list',filter:value.brandon.filter??defaultTaskFilter()});if(filter.error)throw Error(filter.error)
  const views=value.views.map((v,i):TaskFormatView=>{
    if(!object(v)||typeof v.id!=='string'||!v.id||v.id.length>128||ids.has(v.id)||typeof v.name!=='string'||!v.name.trim()||v.name.length>128)throw Error('Некорректное или повторяющееся представление')
    ids.add(v.id)
    const layouts:Record<string,TaskFormatLayout>={'brandon-task-table':'table','brandon-task-kanban':'kanban','brandon-task-timeline':'timeline'}
    const layout=typeof v.type==='string'&&Object.hasOwn(layouts,v.type)?layouts[v.type]:undefined
    if(!layout)throw Error('Неизвестный тип task view; данные не подменены заметками')
    const parsed=parseTaskView({id:v.id,name:v.name,mode:layout==='kanban'?'kanban':'list',sort:v.sort});if(parsed.error)throw Error(parsed.error)
    if(v.windowDays!==undefined&&![7,14,30,60].includes(v.windowDays as number))throw Error('Окно Timeline: 7, 14, 30 или 60 дней')
    ignored(v,['id','name','type','sort','windowDays'],`views[${i}].`)
    return{id:v.id,name:v.name,layout,sort:parsed.sort,windowDays:(v.windowDays??14) as TaskFormatView['windowDays']}
  })
  if(ignoredCount>warnings.length)warnings.push(`Ещё ${ignoredCount-warnings.length} неизвестных полей сохранено; список предупреждений ограничен, исходник не обрезан.`)
  return{filter:filter.filter,views,warnings,raw:value}
}
export function createTaskFormat(project='',layout:TaskFormatLayout='table'){
  const layouts:TaskFormatLayout[]=[layout,...(['table','kanban','timeline'] as const).filter(v=>v!==layout)]
  return JSON.stringify({brandon:{version:1,format:'task-view',filter:{...defaultTaskFilter(),project}},views:layouts.map(v=>({id:v,name:{table:'Таблица',kanban:'Kanban',timeline:'Timeline'}[v],type:`brandon-task-${v}`,...(v==='timeline'?{windowDays:14}:{})}))},null,2)+'\n'
}

const DAY=86400000
/** UTC methods encode civil dates for arithmetic only; no timezone/instant conversion. */
export function civilOrdinal(value:string){
  if(!/^\d{4}-\d{2}-\d{2}$/.test(value))throw Error('Дата должна быть ГГГГ-ММ-ДД')
  const [year,month,day]=value.split('-').map(Number),date=new Date(0);date.setUTCFullYear(year,month-1,day);date.setUTCHours(0,0,0,0)
  if(year<1||year>9999||date.getUTCFullYear()!==year||date.getUTCMonth()!==month-1||date.getUTCDate()!==day)throw Error('Некорректная календарная дата')
  return date.getTime()/DAY
}
export function civilDate(ordinal:number){
  if(!Number.isInteger(ordinal)||Math.abs(ordinal)>4000000)throw Error('Дата вне допустимого диапазона')
  const date=new Date(ordinal*DAY),year=date.getUTCFullYear();if(year<1||year>9999)throw Error('Дата вне 0001..9999')
  return`${String(year).padStart(4,'0')}-${String(date.getUTCMonth()+1).padStart(2,'0')}-${String(date.getUTCDate()).padStart(2,'0')}`
}
export function civilSchedule(value:string){
  if(!/^\d{4}-\d{2}-\d{2}(?:[ T](?:[01]\d|2[0-3]):[0-5]\d)?$/.test(value))throw Error('Некорректная локальная дата/время задачи')
  return{day:civilOrdinal(value.slice(0,10)),minutes:value.length===16?Number(value.slice(11,13))*60+Number(value.slice(14,16)):null}
}
export function shiftTaskSchedule(task:Pick<TaskMetadata,'scheduled'>,days:number):TaskPatch{
  if(!task.scheduled)throw Error('Сначала задайте плановую дату')
  if(!Number.isInteger(days)||Math.abs(days)>36500)throw Error('Некорректный сдвиг плана')
  return{scheduled:civilDate(civilSchedule(task.scheduled).day+days)+task.scheduled.slice(10)}
}
export function resizeTaskDuration(task:Pick<TaskMetadata,'scheduled'|'durationMinutes'>,deltaMinutes:number):TaskPatch{
  if(!task.scheduled||civilSchedule(task.scheduled).minutes===null)throw Error('Для изменения интервала перетаскиванием задайте время; дату без времени не превращаем в полночь')
  if(!Number.isInteger(deltaMinutes)||deltaMinutes%15!==0)throw Error('Изменение длительности — шагом 15 минут')
  const next=(task.durationMinutes??60)+deltaMinutes
  if(next<1||next>525600)throw Error('Длительность — от 1 до 525600 минут')
  return{durationMinutes:next}
}
export function timelineTask(task:Pick<TaskMetadata,'scheduled'|'due'|'durationMinutes'>,start:string,days:number){
  if(![7,14,30,60].includes(days))throw Error('Некорректное окно Timeline')
  if(task.durationMinutes!==null&&(!Number.isInteger(task.durationMinutes)||task.durationMinutes<1||task.durationMinutes>525600))throw Error('Некорректная длительность задачи')
  const origin=civilOrdinal(start),due=task.due?civilSchedule(task.due):null,dueAt=due?due.day-origin+(due.minutes??0)/1440:null
  const marker=dueAt!==null&&dueAt>=0&&dueAt<days?dueAt/days*100:null
  if(!task.scheduled)return{bar:null,due:marker,reason:'unscheduled' as const}
  const plan=civilSchedule(task.scheduled),x=plan.day-origin+(plan.minutes??0)/1440,duration=plan.minutes===null?1:(task.durationMinutes??60)/1440
  if(x>=days||x+duration<=0)return{bar:null,due:marker,reason:'outside' as const}
  const left=Math.max(0,x),right=Math.min(days,x+duration)
  return{bar:{left:left/days*100,width:(right-left)/days*100,dateOnly:plan.minutes===null,clipped:x<0||x+duration>days},due:marker,reason:null}
}
