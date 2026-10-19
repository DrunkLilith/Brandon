// Reproducible 10k/50k real-app work/latency measurement, not a query-only timer.
import assert from 'node:assert/strict'
import {mkdtemp,mkdir,writeFile,readFile} from 'node:fs/promises'
import {join,resolve} from 'node:path'
import {performance} from 'node:perf_hooks'
import {connect} from './cdp-test-client.mjs'
const c=await connect(),{evaluate,send,click,key,delay}=c
const command=(name,args={})=>evaluate(`window.__TAURI_INTERNALS__.invoke(${JSON.stringify(name)},${JSON.stringify(args)})`)
async function until(expression,timeout=180000){const at=performance.now();while(performance.now()-at<timeout){if(await evaluate(expression))return;await delay(80)}throw Error(`Benchmark timeout: ${expression}`)}
const original=await command('restore_vault');assert.ok(original?.root)
assert.equal(await evaluate("Boolean(document.querySelector('.title-tab.active i,.side-save:not(:disabled),.conflict-dialog,.source-recovery,.workspace-dialog'))"),false)
const output=resolve('target/qa'),reports=[]
const percentile=(values,q)=>[...values].sort((a,b)=>a-b)[Math.min(values.length-1,Math.floor(q*values.length))]??null
try{
  for(const count of [10000,50000]){
    const noteCount=count-1 // one .base is also a resource under the 50k FTS cap
    const fixture=await mkdtemp(join(output,`metadata-${count}-`)),notes=join(fixture,'Notes');await mkdir(notes)
    let next=0
    await Promise.all(Array.from({length:48},async()=>{while(next<noteCount){const i=next++;await writeFile(join(notes,`N${String(i).padStart(5,'0')}.md`),`---\nstatus: active\n---\n# Note ${i}\n\nA small reproducible source. #common [[N00000]]\n`)}}))
    await writeFile(join(fixture,'View.base'),'views:\n  - type: table\n')
    const openedAt=performance.now(),snapshot=await command('open_vault',{path:fixture}),inventoryMs=performance.now()-openedAt,root=snapshot.root,generation=snapshot.metadataGeneration
    await send('Page.reload')
    await until(`document.querySelector('.metadata-status')?.dataset.pending==='0' && window.__TAURI_INTERNALS__.invoke('metadata_status',{root:${JSON.stringify(root)}}).then(delta=>delta.stats.documents===${noteCount}&&delta.stats.pendingSources===0)`)
    const warmMs=performance.now()-openedAt
    await until(`window.__TAURI_INTERNALS__.invoke('search_vault',{root:${JSON.stringify(root)},query:'r2awarmupmissingmarker',mode:'words',limit:1}).then(result=>!result.syncing&&result.indexedSources===${noteCount}&&result.nameOnlySources===1)`)
    const searchWarmMs=performance.now()-openedAt
    await click('.content-shell button[title="Редактор"]');await until("Boolean(document.querySelector('.content-shell .cm-content'))")
    await evaluate('new Promise(resolve=>requestAnimationFrame(()=>requestAnimationFrame(resolve)))')
    const treeDom=await evaluate("document.querySelectorAll('.tree-row').length");assert.ok(treeDom<100)
    const treeGeometry=await evaluate("({height:document.querySelector('.tree-list').getBoundingClientRect().height,rowHeight:document.querySelector('.tree-row').getBoundingClientRect().height,total:Number(document.querySelector('.tree-list').dataset.totalRows)})")
    assert.ok(Math.abs(treeGeometry.height-treeGeometry.total*treeGeometry.rowHeight)<2,'Virtual scroll height must match the current CSS row height')
    await evaluate("window.__r2aFrames=[];window.__r2aFrameAt=0;window.__r2aMeasure=true;requestAnimationFrame(function tick(t){if(!window.__r2aMeasure)return;if(window.__r2aFrameAt)window.__r2aFrames.push(t-window.__r2aFrameAt);window.__r2aFrameAt=t;requestAnimationFrame(tick)})")
    await evaluate("document.querySelector('.content-shell .cm-content').focus()");await key('End','End',2)
    const inputAt=performance.now();await send('Input.insertText',{text:'\nBENCH_TYPED_INPUT'})
    await until("document.querySelector('.content-shell .cm-content').textContent.includes('BENCH_TYPED_INPUT')")
    await evaluate('new Promise(resolve=>requestAnimationFrame(()=>requestAnimationFrame(resolve)))')
    const inputToPaintMs=performance.now()-inputAt;await key('s','KeyS',2)
    await until("document.querySelector('.save-button')?.disabled===true && document.querySelector('.metadata-status')?.dataset.pending==='0'")
    const status=()=>command('metadata_status',{root})
    await until(`window.__TAURI_INTERNALS__.invoke('metadata_status',{root:${JSON.stringify(root)}}).then(delta=>delta.stats.parsedSources===${noteCount+1}&&delta.stats.pendingSources===0)`)
    const edits=[]
    for(let i=0;i<3;i++){
      const before=await status(),path=join(notes,'N00000.md'),source=await readFile(path,'utf8'),marker=`EXTERNAL_ROUND_${i}`,at=performance.now()
      await writeFile(path,`${source}\n${marker}\n`)
      await until(`document.querySelector('.content-shell .cm-content').textContent.includes(${JSON.stringify(marker)}) && window.__TAURI_INTERNALS__.invoke('metadata_delta',{root:${JSON.stringify(root)},generation:${generation},after:${before.revision}}).then(delta=>delta.stats.parsedSources===${before.stats.parsedSources+1}&&delta.stats.pendingSources===0)`)
      await evaluate('new Promise(resolve=>requestAnimationFrame(()=>requestAnimationFrame(resolve)))')
      const endToPaintMs=performance.now()-at,delta=await command('metadata_delta',{root,generation,after:before.revision})
      assert.equal(delta.stats.fullScans,before.stats.fullScans);assert.equal(delta.stats.parsedSources-before.stats.parsedSources,1)
      assert.equal(delta.documents.length,1,'Body-only edits must not resend all incoming-link sources')
      assert.ok(delta.stats.scannedEntries-before.stats.scannedEntries<20)
      edits.push({endToPaintMs,parsedSources:delta.stats.parsedSources-before.stats.parsedSources,readSources:delta.stats.readSources-before.stats.readSources,scannedEntries:delta.stats.scannedEntries-before.stats.scannedEntries,resolvedSources:delta.stats.resolvedSources-before.stats.resolvedSources,fullScans:delta.stats.fullScans-before.stats.fullScans,wireBytes:Buffer.byteLength(JSON.stringify(delta))})
    }
    const frames=await evaluate('window.__r2aMeasure=false;window.__r2aFrames')
    await evaluate("document.querySelector('.tree-row[title=\"Notes/N00000.md\"]').focus()");await key('End','End')
    await until("document.activeElement?.title==='View.base'")
    assert.ok(await evaluate("document.querySelectorAll('.tree-row').length<100"))
    await click('.tree-row[title="View.base"]');await until("Boolean(document.querySelector('.base-table-wrap'))")
    assert.equal(await evaluate("Number(document.querySelector('.base-table-wrap').dataset.totalRows)"),noteCount,'Bootstrap must supply every parsed document, not just a warm counter')
    const baseDom=await evaluate("document.querySelectorAll('.base-table tbody tr:not([aria-hidden])').length");assert.ok(baseDom<100)
    await evaluate("document.querySelector('.base-toolbar input').focus()")
    const filterAt=performance.now();await send('Input.insertText',{text:`Note ${noteCount-1}`});await until("document.querySelectorAll('.base-table tbody tr:not([aria-hidden])').length===1")
    const filterMs=performance.now()-filterAt
    const report={count,noteCount,fixture,inventoryMs,warmMs,searchWarmMs,inputToPaintMs,filterMs,treeDom,baseDom,treeGeometry,frameIntervalsMs:{p50:percentile(frames,.5),p95:percentile(frames,.95),max:Math.max(...frames)},edits,notes:'Local synthetic Windows/Tauri/WebView2 run. Count is total resources: noteCount Markdown plus one .base. UI timings include CDP polling/paint overhead; no universal latency guarantee. Both metadata and search workers are enabled and fully warmed before timed edits. Generated files have a warm OS cache. Fixtures retained under ignored target/qa.'}
    reports.push(report);console.log(JSON.stringify(report));await writeFile(join(output,'metadata-benchmark.json'),JSON.stringify({date:new Date().toISOString(),reports},null,2))
  }
}finally{
  await evaluate('window.__r2aMeasure=false').catch(()=>{})
  await command('open_vault',{path:original.root});await send('Page.reload');c.socket.close()
}
