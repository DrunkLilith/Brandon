import assert from 'node:assert/strict'
import {mkdtemp,writeFile,readFile,mkdir,rm} from 'node:fs/promises'
import {tmpdir} from 'node:os'
import {join,resolve,sep} from 'node:path'
import {crc32,deflateSync} from 'node:zlib'
import {connect} from './cdp-test-client.mjs'
const {socket,send,evaluate,waitFor,click,key,delay}=await connect()
const original=await evaluate("window.__TAURI_INTERNALS__.invoke('restore_vault')")
assert.equal(await evaluate("Boolean(document.querySelector('.title-tab.active i,.conflict-dialog,.workspace-dialog'))"),false)
const scratch=await mkdtemp(join(tmpdir(),'brandon-card-ui-'))
const passed=[],pass=text=>{passed.push(text);console.log(`PASS ${text}`)}
function png(){const w=1200,h=700,raw=Buffer.alloc(h*(1+w*3));for(let y=0;y<h;y++)for(let x=0;x<w;x++){const i=y*(1+w*3)+1+x*3;raw[i]=80+x/w*150;raw[i+1]=50+y/h*100;raw[i+2]=180}const chunk=(type,data)=>{const name=Buffer.from(type),head=Buffer.alloc(4),tail=Buffer.alloc(4);head.writeUInt32BE(data.length);tail.writeUInt32BE(crc32(Buffer.concat([name,data])));return Buffer.concat([head,name,data,tail])};const header=Buffer.alloc(13);header.writeUInt32BE(w);header.writeUInt32BE(h,4);header[8]=8;header[9]=2;return Buffer.concat([Buffer.from('89504e470d0a1a0a','hex'),chunk('IHDR',header),chunk('IDAT',deflateSync(raw)),chunk('IEND',Buffer.alloc(0))])}
async function pointer(selector,button='left'){const point=await evaluate(`(()=>{const r=document.querySelector(${JSON.stringify(selector)}).getBoundingClientRect();return{x:r.left+r.width/2,y:r.top+r.height/2}})()`);await send('Input.dispatchMouseEvent',{type:'mouseMoved',...point});await send('Input.dispatchMouseEvent',{type:'mousePressed',button,buttons:button==='right'?2:1,clickCount:1,...point});await send('Input.dispatchMouseEvent',{type:'mouseReleased',button,buttons:0,clickCount:1,...point})}
try{
  await mkdir(join(scratch,'Attachments'));const image=png();await writeFile(join(scratch,'Attachments','Color.png'),image)
  await writeFile(join(scratch,'Note.md'),'# Note\n\nA linked note.\n')
  const board={nodes:[{id:'text',type:'text',x:-390,y:-140,width:360,height:280,text:'# A place for ideas\n\nA **clear heading**, useful text and a little breathing room.\n\n- Keep the source\n- Connect the thought'},{id:'image',type:'file',x:30,y:-140,width:360,height:280,file:'Attachments/Color.png'}],edges:[]}
  await writeFile(join(scratch,'Board.canvas'),JSON.stringify(board))
  await evaluate(`window.__TAURI_INTERNALS__.invoke('open_vault',{path:${JSON.stringify(scratch)}})`);await send('Page.reload');await waitFor("Boolean(document.querySelector('.tree-row[title=\"Board.canvas\"]'))")
  await click('.tree-row[title="Board.canvas"]');await waitFor("document.querySelector('.canvas-view')?.dataset.renderer==='wgpu'")
  await waitFor("Boolean(document.querySelector('[data-node-id=\"image\"] img'))")
  await pointer('[data-node-id="image"] .canvas-file-card')
  await waitFor("document.querySelector('.image-viewer img')?.naturalWidth>0")
  assert.equal(await evaluate("document.querySelector('.image-viewer img').naturalWidth"),1200)
  pass('A single real mouse click opens the image from the native GPU board at useful preview resolution')
  await click('button[aria-label="Увеличить изображение"]');assert.equal(await evaluate("document.querySelector('.image-viewer nav button:nth-child(2)').textContent"),'125%')
  await key('Escape','Escape');await waitFor("!document.querySelector('.image-viewer')")
  assert.deepEqual(await readFile(join(scratch,'Attachments','Color.png')),image)
  pass('Image viewer zoom and Escape work without modifying the original image')
  await click('.tree-row[title="Attachments/Color.png"]');await waitFor("document.querySelector('.image-viewer img')?.naturalWidth>0");await key('Escape','Escape')
  pass('The same image opens directly from the vault file tree')
  assert.ok(await evaluate("document.querySelector('[data-node-id=\"text\"] .canvas-rich-text h1')?.textContent==='A place for ideas' && document.querySelector('[data-node-id=\"text\"] .canvas-rich-text strong')?.textContent==='clear heading'"))
  pass('Small native boards render text cards as formatted Markdown instead of raw source')
  await pointer('[data-node-id="text"]','right');await waitFor("Boolean(document.querySelector('.command-menu'))")
  await key('ArrowDown','ArrowDown');await key('Enter','Enter')
  await waitFor("document.querySelector('.canvas-view')?.dataset.totalNodes==='3'")
  await key('z','KeyZ',2);await waitFor("document.querySelector('.canvas-view')?.dataset.totalNodes==='2'")
  pass('Actual right-click menu duplicates a card and one Undo reverses the action')
  await pointer('[data-node-id="image"]','right');await waitFor("Boolean(document.querySelector('.command-menu'))")
  await evaluate("Array.from(document.querySelectorAll('.command-menu button')).find(b=>b.textContent.trim()==='Открыть').click()")
  await waitFor("Boolean(document.querySelector('.image-viewer img'))");await key('Escape','Escape')
  pass('The same context menu opens a selected attachment without touching file bytes')
  await delay(1000);await waitFor("!document.querySelector('.title-tab.active i')")
  if(process.env.BRANDON_QA_SCREENSHOT){const shot=await send('Page.captureScreenshot',{format:'png'});await writeFile(process.env.BRANDON_QA_SCREENSHOT,Buffer.from(shot.data,'base64'))}
  console.log(JSON.stringify({passed:passed.length,renderer:'wgpu with bounded rich-card overlay'}))
}finally{await evaluate(`window.__TAURI_INTERNALS__.invoke('open_vault',{path:${JSON.stringify(original.root)}})`);await send('Page.reload');await delay(400);socket.close();assert.ok(resolve(scratch).startsWith(resolve(tmpdir())+sep)&&scratch.includes('brandon-card-ui-'));await rm(scratch,{recursive:true,force:true})}
