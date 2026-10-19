// Independently implemented DQL subset; upstream contracts/revisions are in
// docs/plugin-compatibility-r2b.md. Never loads or executes a plugin/JavaScript.
import {compileExpression,noteRow,type QueryRow} from './query-service.ts'
import type {DocumentMetadata,TaskMetadata,VaultIndex} from './types'
export type DqlColumn={label:string;link:boolean;read:(row:QueryRow)=>unknown}
export type DqlRow={id:string;path:string;line:number|null;task:TaskMetadata|null;values:unknown[]}
export type DqlResult={kind:'TABLE'|'LIST'|'TASK';columns:DqlColumn[];rows:DqlRow[]}
type Entry={document:DocumentMetadata;task:TaskMetadata|null;adapter:QueryRow}
const fileFields=new Set(['file.name','file.path','file.folder','file.ext','file.tags'])
// DQL scalar ordering/truthiness differ from Base semantics. These adapters
// retain the shared bounded interpreter without pretending both languages match.
const truthy=(value:unknown)=>Array.isArray(value)?value.length>0:value!==null&&typeof value==='object'?Object.keys(value).length>0:Boolean(value)
function compare(a:unknown,b:unknown):number{
  if(a==null||b==null)return a==null&&b==null?0:a==null?-1:1
  const type=(value:unknown)=>Array.isArray(value)?'array':typeof value
  if(type(a)!==type(b))return type(a).localeCompare(type(b))
  if(typeof a==='number'&&typeof b==='number')return a-b
  if(typeof a==='string'&&typeof b==='string')return a.localeCompare(b)
  if(typeof a==='boolean'&&typeof b==='boolean')return Number(a)-Number(b)
  throw new Error('Сравнение или сортировка списков/объектов в DQL пока не поддерживается')
}
function field(name:string){
  if(name.startsWith('file.')){if(!fileFields.has(name))throw new Error(`Поле DQL пока не поддерживается: ${name}`);return name==='file.link'?'file.path':name}
  if(!/^[\p{L}_][\p{L}\p{N}_-]*$/u.test(name)||['__proto__','constructor','prototype'].includes(name))throw new Error(`Неподдерживаемое поле DQL: ${name}`)
  return `note[${JSON.stringify(name)}]`
}
export function dqlExpression(source:string){
  if(source.length>4096)throw new Error('Выражение DQL длиннее 4096 символов')
  let out='',i=0
  while(i<source.length){
    const rest=source.slice(i),space=/^\s+/.exec(rest),string=/^(?:"(?:\\.|[^"\\])*"|'(?:\\.|[^'\\])*')/.exec(rest),number=/^-?(?:\d+\.?\d*|\.\d+)(?:[eE][+-]?\d+)?/.exec(rest),word=/^[\p{L}_][\p{L}\p{N}_.-]*/u.exec(rest),symbol=/^(?:!=|>=|<=|==|<>|=|&&|\|\||[!><()])/.exec(rest)
    if(space){out+=' ';i+=space[0].length}
    else if(string){out+=string[0];i+=string[0].length}
    else if(number){out+=number[0];i+=number[0].length}
    else if(word){const value=word[0],keyword=value.toLowerCase();i+=value.length;if(!['and','or','not'].includes(keyword)&&/^\s*\(/.test(source.slice(i)))throw new Error(`Функция DQL пока не поддерживается: ${value}`);out+=keyword==='and'?'&&':keyword==='or'?'||':keyword==='not'?'!':['true','false','null'].includes(keyword)?keyword:field(value)}
    else if(symbol){out+=symbol[0]==='='?'==':symbol[0]==='<>'?'!=':symbol[0];i+=symbol[0].length}
    else throw new Error(`Неподдерживаемый синтаксис DQL: ${rest.slice(0,24)}`)
  }
  return compileExpression(out,undefined,undefined,{truthy,compare})
}
function split(source:string):string[]{
  const parts:string[]=[];let start=0,quote='',escaped=false,depth=0
  for(let i=0;i<source.length;i++){const ch=source[i];if(quote){if(escaped)escaped=false;else if(ch==='\\')escaped=true;else if(ch===quote)quote=''}else if(ch==='"'||ch==="'")quote=ch;else if(ch==='('){if(++depth>32)throw new Error('Слишком глубокий DQL')}else if(ch===')'){if(--depth<0)throw new Error('Лишняя скобка DQL')}else if(ch===','&&depth===0){parts.push(source.slice(start,i).trim());start=i+1}}
  if(quote||depth)throw new Error('Не закрыты строка или скобки DQL');parts.push(source.slice(start).trim());if(parts.length>32)throw new Error('Не более 32 полей DQL');return parts
}
function quoted(source:string){if(!/^"(?:\\.|[^"\\])*"$/.test(source))throw new Error('Здесь ожидается строка в двойных кавычках');try{return JSON.parse(source) as string}catch{throw new Error('Некорректная строка DQL')}}
function column(source:string):DqlColumn{
  const alias=/^([\s\S]+?)\s+AS\s+("(?:\\.|[^"\\])*")$/i.exec(source),expression=alias?.[1]??source
  if(!expression)throw new Error('Пустое поле DQL')
  const link=expression.trim()==='file.link'
  return {label:alias?quoted(alias[2]):expression,link,read:link?row=>row.field('file.path'):dqlExpression(expression)}
}
function adapter(document:DocumentMetadata,task:TaskMetadata|null):QueryRow{
  const page=noteRow(document)
  return {...page,field:key=>{if(task){switch(key){case 'note.completed':return task.completed;case 'note.text':return task.title;case 'note.due':return task.due;case 'note.scheduled':return task.scheduled;case 'note.priority':return task.priority}}return page.field(key)}}
}
export function runDql(source:string,index:VaultIndex):DqlResult{
  if(source.length>16384)throw new Error('DQL длиннее 16 КиБ')
  const lines=source.split(/\r?\n/).map(line=>line.trim()).filter(Boolean)
  if(!lines.length||lines.length>64)throw new Error('DQL: от 1 до 64 непустых строк')
  const head=/^(TABLE|LIST|TASK)(?:\s+([\s\S]*))?$/i.exec(lines[0]);if(!head)throw new Error('Поддерживаются только TABLE, LIST и read-only TASK')
  const kind=head[1].toUpperCase() as DqlResult['kind'];let fields=head[2]??'',withoutId=false
  if(kind==='TABLE'&&/^WITHOUT ID(?:\s|$)/i.test(fields)){withoutId=true;fields=fields.replace(/^WITHOUT ID\s*/i,'')}
  if(kind==='TASK'&&fields)throw new Error('TASK не принимает список полей')
  const columns=kind==='TABLE'?fields?split(fields).map(column):[]:kind==='LIST'&&fields?[column(fields)]:[]
  if(kind==='TABLE'&&!withoutId)columns.unshift({label:'File',link:true,read:row=>row.field('file.path')})
  if(kind==='TABLE'&&!columns.length)throw new Error('TABLE WITHOUT ID требует хотя бы одно поле')
  let sourceTest:(row:QueryRow)=>boolean=()=>true,start=1
  if(/^FROM\b/i.test(lines[1]??'')){
    const value=lines[1].replace(/^FROM\s+/i,'')
    if(/^#[\p{L}\p{N}_/-]+$/u.test(value)){const tag=value.slice(1);sourceTest=row=>row.hasTag(tag)}
    else {const folder=quoted(value);if(folder.startsWith('/')||folder.includes('\\')||folder.split('/').some(part=>part==='..'||part==='.')||folder.includes('\u0000'))throw new Error('Источник DQL должен быть относительной папкой vault');sourceTest=row=>row.inFolder(folder)}
    start=2
  }
  // Compile every clause before evaluating anything: unsupported syntax never
  // falls back to an unfiltered result, even for an empty source.
  const steps=lines.slice(start).map(line=>{
    const match=/^(WHERE|SORT|LIMIT)\s+([\s\S]+)$/i.exec(line);if(!match)throw new Error(`Команда DQL пока не поддерживается: ${line.slice(0,40)}`)
    const body=match[2]
    if(match[1].toUpperCase()==='WHERE'){const test=dqlExpression(body);return (entries:Entry[])=>entries.filter(entry=>truthy(test(entry.adapter)))}
    if(match[1].toUpperCase()==='LIMIT'){if(!/^\d+$/.test(body)||Number(body)>50000)throw new Error('LIMIT: целое число от 0 до 50000');return (entries:Entry[])=>entries.slice(0,Number(body))}
    const sorts=split(body).map(value=>{const match=/^([\s\S]*?)(?:\s+(ASC|DESC|ASCENDING|DESCENDING))?$/i.exec(value)!;return {read:dqlExpression(match[1]),direction:/^DESC/i.test(match[2]??'')?-1:1}})
    return (entries:Entry[])=>entries.sort((a,b)=>{for(const sort of sorts){const compared=compare(sort.read(a.adapter),sort.read(b.adapter));if(compared)return compared*sort.direction}return 0})
  })
  if(index.documents.length>50000)throw new Error('DQL ограничен 50000 исходными заметками')
  let entries:Entry[]=[]
  for(const document of index.documents){const page=adapter(document,null);if(!sourceTest(page))continue;for(const task of kind==='TASK'?document.tasks:[null]){entries.push({document,task,adapter:adapter(document,task)});if(entries.length>50000)throw new Error('DQL ограничен 50000 исходными строками')}}
  for(const step of steps)entries=step(entries)
  return {kind,columns,rows:entries.map(entry=>({id:entry.task?.id??entry.document.path,path:entry.document.path,line:entry.task?.line??null,task:entry.task,values:columns.map(column=>column.read(entry.adapter))}))}
}
