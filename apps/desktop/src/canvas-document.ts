import type { CanvasDocument, CanvasNode } from './canvas-view'

// JSON.parse would silently round a foreign 64-bit ID or a high-precision
// decimal, then stringify would persist the changed value on an unrelated move.
function validateNumbers(source:string){
  const decimal=(token:string)=>{
    const [mantissa,exponent='0']=token.toLowerCase().split('e'),negative=mantissa.startsWith('-'),[whole,fraction='']=mantissa.replace(/^-/,'').split('.')
    const digits=(whole+fraction).replace(/^0+/,''),compact=digits.replace(/0+$/,'')
    return compact?`${negative?'-':''}${compact}e${Number(exponent)-fraction.length+digits.length-compact.length}`:'0'
  }
  for(const match of source.matchAll(/"(?:[^"\\]|\\.)*"|(-?\d+(?:\.\d+)?(?:[eE][+-]?\d+)?)/g)){
    const token=match[1];if(token===undefined)continue
    const value=Number(token)
    if(!Number.isFinite(value)||Number.isInteger(value)&&!Number.isSafeInteger(value)||decimal(token)!==decimal(String(value)))throw Error('Число в Canvas нельзя сохранить без потери точности; исходный файл доступен только для чтения')
  }
}

function validateExtension(value:unknown,edge=false){
  if(value===undefined)return
  if(!value||typeof value!=='object'||Array.isArray(value))throw Error('Расширение Brandon не является объектом; файл доступен только для чтения')
  const extension=value as Record<string,unknown>
  if(extension.version!==1)throw Error('Неизвестная версия расширения Brandon; исходный файл не изменён')
  const enums:Record<string,unknown[]>=edge?{dash:['solid','dashed'],strokeWidth:[1,2,4]}:{primitive:['sticky','text','rectangle','ellipse','diamond','triangle','divider'],fontSize:[12,16,24,32],textAlign:['left','center','right'],strokeWidth:[1,2,4]}
  for(const [key,values]of Object.entries(enums))if(extension[key]!==undefined&&!values.includes(extension[key]))throw Error(`Неизвестное значение Brandon.${key}; исходный файл не изменён`)
  if(!edge&&extension.locked!==undefined&&typeof extension.locked!=='boolean')throw Error('Некорректная блокировка Brandon')
  if(!edge)for(const key of ['fillColor','strokeColor','textColor'])if(extension[key]!==undefined&&!(key==='fillColor'&&extension[key]==='transparent')&&!(typeof extension[key]==='string'&&/^#[\da-f]{6}$/i.test(extension[key] as string)))throw Error(`Некорректный цвет Brandon.${key}`)
}

export function parseCanvasDocument(source: string): { document: CanvasDocument; error: string | null } {
  try {
    validateNumbers(source)
    const value: unknown = JSON.parse(source)
    if (!value || typeof value !== 'object' || Array.isArray(value)) throw new Error('Ожидается объект JSON Canvas')
    const document = value as CanvasDocument
    if (!Array.isArray(document.nodes) || !Array.isArray(document.edges)) throw new Error('Нужны массивы nodes и edges')
    const ids = new Set<string>()
    for (const node of document.nodes) {
      if (!node || typeof node.id !== 'string' || !node.id || ids.has(node.id)) throw new Error('Пустой или повторяющийся ID карточки')
      ids.add(node.id)
      if (!['text', 'file', 'link', 'group'].includes(node.type)) throw new Error('Неизвестный тип карточки')
      if (![node.x, node.y, node.width, node.height].every((v) => typeof v === 'number' && Number.isFinite(v) && Math.abs(v) <= 10_000_000) || node.width <= 0 || node.height <= 0) throw new Error('Некорректные координаты или размер карточки')
      for (const key of ['text', 'file', 'url', 'label', 'color'] as const) if (node[key] !== undefined && typeof node[key] !== 'string') throw new Error(`Поле ${key} должно быть строкой`)
      validateExtension(node.brandon)
      if(node.type!=='text'&&node.brandon&&typeof node.brandon==='object'&&'primitive' in node.brandon&&(node.brandon as {primitive?:unknown}).primitive!==undefined)throw Error('Фигура Brandon требует текстовый fallback')
    }
    const edgeIds = new Set<string>()
    for (const edge of document.edges) {
      if (!edge || typeof edge.id !== 'string' || !edge.id || edgeIds.has(edge.id) || typeof edge.fromNode !== 'string' || typeof edge.toNode !== 'string') throw new Error('Некорректная связь или повторяющийся ID')
      edgeIds.add(edge.id)
      for(const key of ['fromSide','toSide']as const)if(edge[key]!==undefined&&!['top','right','bottom','left'].includes(edge[key]!))throw Error(`Некорректная сторона связи ${key}`)
      for(const key of ['fromEnd','toEnd']as const)if(edge[key]!==undefined&&!['none','arrow'].includes(edge[key]!))throw Error(`Некорректный конец связи ${key}`)
      for(const key of ['color','label']as const)if(edge[key]!==undefined&&typeof edge[key]!=='string')throw Error(`Поле связи ${key} должно быть строкой`)
      validateExtension(edge.brandon,true)
    }
    return { document, error: null }
  } catch (error) {
    return { document: { nodes: [], edges: [] }, error: error instanceof Error ? error.message : 'Некорректный JSON Canvas' }
  }
}

export function canvasColors(node: Pick<CanvasNode, 'type' | 'color'>) {
  const fill=(color:string)=>node.type==='group'?`${color}33`:color
  const palettes: Record<string, [string, string]> = {
    '1': ['#38272a', '#a96770'], '2': ['#392d25', '#b58259'], '3': ['#363225', '#a89a58'],
    '4': ['#26352c', '#649776'], '5': ['#253339', '#5e97a7'], '6': ['#322b3e', '#9381ba'],
  }
  if (node.color && palettes[node.color]) return { fill: fill(palettes[node.color][0]), border: palettes[node.color][1] }
  if (node.color && /^#[\da-f]{6}$/i.test(node.color)) {
    const value = Number.parseInt(node.color.slice(1), 16)
    const parts = [value >> 16, value >> 8 & 255, value & 255].map((channel) => Math.round(channel * .18 + 30 * .82).toString(16).padStart(2, '0'))
    return { fill: fill(`#${parts.join('')}`), border: node.color }
  }
  return { fill: fill(node.type === 'group' ? '#211e27' : node.type === 'file' ? '#292531' : '#2b2732'), border: '#51465f' }
}
