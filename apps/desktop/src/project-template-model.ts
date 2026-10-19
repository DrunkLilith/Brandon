import {parseCanvasDocument} from './canvas-document.ts'
import {compileBase} from './query-service.ts'
import {compileTaskFormat,isWorkingFormat} from './working-formats.ts'
export type ProjectTemplateChoice={id:string;name:string;category:string;description:string;entrypoint:string;files:string[];fingerprint:string;builtin:boolean;error:string|null}
export type ProjectTemplateCatalog={fingerprint:string|null;choices:ProjectTemplateChoice[];error:string|null}
export type ProjectTemplateInput={id:string;fingerprint:string;name:string;parent:string;date:string;requestId:string}
export type ProjectTemplatePreview={input:ProjectTemplateInput;path:string;entrypoint:string;projectKey:string;files:{path:string;content:string;fingerprint:string}[];warnings:string[];planHash:string}
export type ProjectTemplateCreated={path:string;entrypoint:string;projectKey:string;created:boolean;files:number}
export type ProjectTemplateEdit={name:string;category:string;description:string;entrypoint:string;files:{path:string;content:string}[]}
const object=(v:unknown):v is Record<string,unknown>=>Boolean(v)&&typeof v==='object'&&!Array.isArray(v)
/** Read known form fields only; never stringify a parsed foreign source back to storage. */
export function projectTemplateEdit(source:string):ProjectTemplateEdit{
  const v:unknown=JSON.parse(source)
  if(!object(v)||v.format!=='brandon.project-template'||v.version!==1||typeof v.name!=='string'||typeof v.category!=='string'||typeof v.description!=='string'||typeof v.entrypoint!=='string'||!Array.isArray(v.files)||v.files.length<1||v.files.length>32)throw Error('Неподдерживаемый источник шаблона')
  const files=v.files.map(f=>{if(!object(f)||typeof f.path!=='string'||typeof f.content!=='string')throw Error('Некорректный файл шаблона');return{path:f.path,content:f.content}})
  return{name:v.name,category:v.category,description:v.description,entrypoint:v.entrypoint,files}
}
export function verifyProjectTemplatePreview(preview:ProjectTemplatePreview){
  if(!preview.files.length||preview.files.length>32)throw Error('Некорректный состав проекта')
  const ids=new Set<string>();let bytes=0
  for(const file of preview.files){
    if(ids.has(file.path.toLocaleLowerCase('ru')))throw Error('Повторяющийся путь проекта');ids.add(file.path.toLocaleLowerCase('ru'))
    const size=new TextEncoder().encode(file.content).length;bytes+=size;if(size>2*1024*1024||bytes>8*1024*1024)throw Error('Превышен бюджет предпросмотра проекта')
    if(/\.canvas$/i.test(file.path)){const result=parseCanvasDocument(file.content);if(result.error)throw Error(`${file.path}: ${result.error}`)}
    else if(/\.base$/i.test(file.path)){const raw:unknown=JSON.parse(file.content);if(isWorkingFormat(raw))compileTaskFormat(raw);else compileBase(raw)}
    else if(!/\.md$/i.test(file.path))throw Error('Неизвестный тип файла проекта')
  }
  return preview
}
export function filterProjectTemplates(choices:ProjectTemplateChoice[],query:string,category:string){
  const needle=query.trim().toLocaleLowerCase('ru');return choices.filter(c=>(!category||c.category===category)&&(!needle||`${c.name} ${c.description} ${c.category}`.toLocaleLowerCase('ru').includes(needle)))
}
