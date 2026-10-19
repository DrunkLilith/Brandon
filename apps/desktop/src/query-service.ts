// Bounded declarative expressions. No eval, Function, JS imports or prototype traversal.
import type { DocumentMetadata } from './types'
export type QueryRow = { field:(key:string)=>unknown;hasTag:(tag:string)=>boolean;inFolder:(folder:string)=>boolean;hasLink:(path:string)=>boolean }
type Token={kind:'id'|'string'|'number'|'symbol';value:string}
type Expression=(row:QueryRow)=>unknown
const forbidden=new Set(['__proto__','prototype','constructor'])
export function propertyKey(key:string) {
  const field=key.startsWith('note.')?key.slice(5):key
  if(!field||field.length>256||field.split('.').some(part=>forbidden.has(part)))throw new Error(`Недоступное поле: ${key}`)
  if(key.startsWith('formula.')||key.startsWith('this.'))throw new Error(`Поле пока не поддерживается: ${key}`)
  if(key.startsWith('file.')&&!['file.name','file.path','file.folder','file.ext','file.tags','file.mtime'].includes(key))throw new Error(`Файловое поле пока не поддерживается: ${key}`)
  return field
}
export function noteRow(note:DocumentMetadata):QueryRow {
  return {field:key=>{
    const name=note.path.split('/').at(-1)??note.path
    switch(key){case 'file.name':return name.replace(/\.md$/i,'');case 'file.path':return note.path;case 'file.folder':return note.path.includes('/')?note.path.slice(0,note.path.lastIndexOf('/')):'';case 'file.ext':return 'md';case 'file.tags':return note.tags;case 'file.mtime':return note.modifiedUnixMs;default:return Object.hasOwn(note.properties,propertyKey(key))?note.properties[propertyKey(key)]:null}
  },hasTag:tag=>note.tags.some(value=>value===tag||value.startsWith(tag+'/')),inFolder:folder=>{const normalized=folder.replace(/^\/+|\/+$/g,'');return !normalized||note.path.startsWith(normalized+'/')},hasLink:path=>note.links.some(link=>link.target===path||link.resolvedPath===path)}
}
function tokens(source:string):Token[] {
  if(source.length>4096)throw new Error('Выражение длиннее 4096 символов')
  const result:Token[]=[];let i=0
  while(i<source.length){
    if(/\s/u.test(source[i])){i++;continue}
    const start=i,ch=source[i]
    if(ch==='"'||ch==="'"){
      i++;let value='',closed=false
      while(i<source.length){const next=source[i++];if(next===ch){closed=true;break}if(next==='\\'){const escape=source[i++];if(escape==='n')value+='\n';else if(escape==='r')value+='\r';else if(escape==='t')value+='\t';else if(escape==='"'||escape==="'"||escape==='\\')value+=escape;else throw new Error('Неподдерживаемая escape-последовательность')}else value+=next}
      if(!closed)throw new Error('Не закрыта строка в выражении');result.push({kind:'string',value})
    }else{
      const remaining=source.slice(i),number=/^-?(?:\d+\.?\d*|\.\d+)(?:[eE][+-]?\d+)?/.exec(remaining),id=/^[\p{L}_][\p{L}\p{N}_.]*/u.exec(remaining),symbol=/^(?:==|!=|>=|<=|&&|\|\||[!><()[\],])/.exec(remaining)
      if(number){result.push({kind:'number',value:number[0]});i+=number[0].length}
      else if(id){result.push({kind:'id',value:id[0]});i+=id[0].length}
      else if(symbol){result.push({kind:'symbol',value:symbol[0]});i+=symbol[0].length}
      else throw new Error(`Неподдерживаемый синтаксис возле «${source.slice(start,start+24)}»`)
    }
    if(result.length>256)throw new Error('Слишком сложное выражение (больше 256 токенов)')
  }
  return result
}
export function compileExpression(source:string,validateField:(field:string)=>unknown=propertyKey,validateFunction:(name:string)=>unknown=()=>{}):Expression {
  const list=tokens(source);let i=0,depth=0
  const accept=(value:string)=>{if(list[i]?.kind==='symbol'&&list[i].value===value){i++;return true}return false}
  const required=(value:string)=>{if(!accept(value))throw new Error(`Ожидается «${value}»`)}
  const primary=():Expression=>{
    if(++depth>32)throw new Error('Слишком глубокое выражение')
    try{
      if(accept('(')){const expr=or();required(')');return expr}
      const token=list[i++];if(!token)throw new Error('Не закончено выражение')
      if(token.kind==='string')return ()=>token.value
      if(token.kind==='number'){const value=Number(token.value);if(!Number.isFinite(value))throw new Error('Некорректное число');return ()=>value}
      if(token.kind!=='id')throw new Error('Ожидается поле или литерал')
      if(token.value==='true'||token.value==='false'||token.value==='null'){const value=token.value==='null'?null:token.value==='true';return ()=>value}
      let field=token.value
      if((field==='note'||field==='file')&&accept('[')){const key=list[i++];if(key?.kind!=='string')throw new Error('Ключ в [] должен быть строкой');required(']');field=field==='note'?`note.${key.value}`:`file.${key.value}`}
      if(accept('(')){
        validateFunction(field)
        if(['file.hasTag','file.inFolder','file.hasLink'].includes(field)){
          const arg=list[i++];if(arg?.kind!=='string')throw new Error('Функция file принимает строковый литерал');required(')');return row=>field==='file.hasTag'?row.hasTag(arg.value):field==='file.inFolder'?row.inFolder(arg.value):row.hasLink(arg.value)
        }
        const method=/^(.*)\.(contains|isEmpty)$/.exec(field)
        if(!method)throw new Error(`Функция не поддерживается: ${field}`)
        propertyKey(method[1]);validateField(method[1]);let value:Expression=()=>null
        if(method[2]==='contains')value=or();required(')')
        return row=>{const target=row.field(method[1]);if(method[2]==='isEmpty')return target==null||target===''||Array.isArray(target)&&target.length===0;const arg=value(row);return typeof target==='string'&&typeof arg==='string'?target.includes(arg):Array.isArray(target)&&target.some(item=>item===arg)}
      }
      propertyKey(field);validateField(field);return row=>row.field(field)
    }finally{depth--}
  }
  const unary=():Expression=>{if(accept('!')){if(++depth>32)throw new Error('Слишком глубокое выражение');const expr=unary();depth--;return row=>!expr(row)}return primary()}
  const compare=():Expression=>{
    const left=unary(),operator=list[i];if(operator?.kind!=='symbol'||!['==','!=','>','<','>=','<='].includes(operator.value))return left
    i++;const right=unary()
    return row=>{const a=left(row),b=right(row);if(operator.value==='==')return a===b;if(operator.value==='!=')return a!==b;if(typeof a!==typeof b||a==null||b==null||!(typeof a==='number'||typeof a==='string'))return false
      const order=typeof a==='number'&&typeof b==='number'?a-b:typeof a==='string'&&typeof b==='string'?(a<b?-1:a>b?1:0):0
      switch(operator.value){case '>':return order>0;case '<':return order<0;case '>=':return order>=0;case '<=':return order<=0;default:return false}}
  }
  const and=():Expression=>{let left=compare();while(accept('&&')){const before=left,right=compare();left=row=>Boolean(before(row))&&Boolean(right(row))}return left}
  const or=():Expression=>{let left=and();while(accept('||')){const before=left,right=and();left=row=>Boolean(before(row))||Boolean(right(row))}return left}
  const expression=or();if(i!==list.length)throw new Error(`Лишний или неподдерживаемый токен: ${list[i].value}`);return expression
}
export function compileFilter(filter:unknown,depth=0):Expression {
  if(depth>24)throw new Error('Фильтр слишком глубоко вложен')
  if(filter===undefined)return ()=>true
  if(typeof filter==='string')return compileExpression(filter)
  if(!filter||typeof filter!=='object'||Array.isArray(filter))throw new Error('Фильтр должен быть строкой или and/or/not')
  const entries=Object.entries(filter);if(entries.length!==1)throw new Error('Ожидается ровно один and, or или not')
  const [operator,items]=entries[0];if(!['and','or','not'].includes(operator)||!Array.isArray(items)||items.length>128)throw new Error('Некорректный логический фильтр')
  if(operator==='not'&&items.length!==1)throw new Error('Поддерживается not с одним вложенным условием')
  const expressions=items.map(item=>compileFilter(item,depth+1));return row=>operator==='or'?expressions.some(expr=>Boolean(expr(row))):operator==='not'?!expressions[0](row):expressions.every(expr=>Boolean(expr(row)))
}
export type BaseDefinition={views:{name:string;columns:string[];sort:{property:string;direction:'ASC'|'DESC'}[];limit:number;test:Expression;error:string|null}[];labels:Record<string,string>;warnings:string[]}
export function compileBase(config:unknown):BaseDefinition {
  const object=(value:unknown):Record<string,unknown>=>value&&typeof value==='object'&&!Array.isArray(value)?value as Record<string,unknown>:{}
  const root=object(config);if(!Array.isArray(root.views)||!root.views.length||root.views.length>64)throw new Error('В .base требуется от 1 до 64 views')
  const warnings:string[]=[],labels:Record<string,string>=Object.create(null)
  for(const [key,value]of Object.entries(object(root.properties))){if(typeof object(value).displayName==='string')labels[key]=object(value).displayName as string}
  for(const key of Object.keys(root))if(!['filters','properties','views'].includes(key))warnings.push(`Раздел «${key}» сохранён в исходнике, но не вычисляется`)
  const views=root.views.map((value,index)=>{
    const view=object(value),name=typeof view.name==='string'?view.name:`Таблица ${index+1}`
    try{
      if(view.type!=='table')throw new Error(`Тип «${String(view.type)}» пока не поддерживается`)
      const columns=view.order===undefined?['file.name','file.path','file.tags']:view.order
      if(!Array.isArray(columns)||columns.length<1||columns.length>64||columns.some(key=>typeof key!=='string'))throw new Error('order: от 1 до 64 имён столбцов')
      for(const key of columns)propertyKey(key)
      if(view.groupBy!==undefined||view.groupOrder!==undefined)throw new Error('Группировка пока не поддерживается')
      const limit=view.limit===undefined?50000:view.limit
      if(typeof limit!=='number'||!Number.isInteger(limit)||limit<0||limit>50000)throw new Error('limit должен быть целым числом от 0 до 50000')
      const sort=view.sort===undefined?[]:view.sort
      if(!Array.isArray(sort)||sort.length>16)throw new Error('sort должен быть списком до 16 полей')
      const sorting=sort.map(value=>{const item=object(value);if(typeof item.property!=='string'||!['ASC','DESC'].includes(String(item.direction)))throw new Error('sort: ожидаются property и direction ASC/DESC');propertyKey(item.property);return {property:item.property,direction:item.direction as 'ASC'|'DESC'}})
      for(const key of Object.keys(view))if(!['name','type','order','sort','limit','filters'].includes(key))warnings.push(`«${name}»: настройка «${key}» сохранена, но не применяется`)
      const globalFilter=compileFilter(root.filters),localFilter=compileFilter(view.filters)
      return {name,columns:columns as string[],sort:sorting,limit,test:(row:QueryRow)=>Boolean(globalFilter(row))&&Boolean(localFilter(row)),error:null}
    }catch(error){return {name,columns:['file.name'],sort:[],limit:0,test:()=>false,error:String(error)}}
  })
  return {views,labels,warnings}
}
export function compareValues(a:unknown,b:unknown,collator:Intl.Collator) {if(a===b)return 0;if(a==null)return 1;if(b==null)return -1;if(typeof a==='number'&&typeof b==='number')return a-b;if(typeof a==='boolean'&&typeof b==='boolean')return Number(a)-Number(b);const text=(value:unknown)=>value!==null&&typeof value==='object'?JSON.stringify(value):String(value);return collator.compare(text(a),text(b))}
