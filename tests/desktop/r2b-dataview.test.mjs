import assert from 'node:assert/strict'
import {test} from 'node:test'
import {runDql,dqlExpression} from '../../apps/desktop/src/dataview-query.ts'
const doc=(path,properties={},tags=[],tasks=[])=>({path,title:path,aliases:[],headings:[],blocks:[],tags,links:[],tasks,properties,wordCount:0,modifiedUnixMs:100})
const task={id:'Projects/A.md::^same',blockId:'same',path:'Projects/A.md',line:6,title:'Source task',completed:false,managed:false,status:'todo',tags:[],taskTags:[],revision:null,project:null,due:null,scheduled:null,priority:null,durationMinutes:null}
const index={documents:[doc('Projects/A.md',{status:'open',rank:20,'recipe-type':'book'},['study/sub'],[task]),doc('Projects/B.md',{status:'done',rank:2},['study']),doc('Outside.md',{status:'open',rank:50},['other'])],tasks:[{...task,id:'canonical-id',managed:true}],tags:{}}
test('DQL TABLE reads real fields, quoted aliases, numeric sorting, folder source and limit',()=>{
  const result=runDql('TABLE status AS "State", rank\nFROM "Projects"\nWHERE status = "open" AND rank >= 10\nSORT rank DESC\nLIMIT 1',index)
  assert.deepEqual(result.columns.map(x=>x.label),['File','State','rank']);assert.deepEqual(result.rows.map(x=>x.values),[['Projects/A.md','open',20]])
})
test('DQL WITHOUT ID and hyphenated property names retain upstream meaning',()=>{const result=runDql('TABLE WITHOUT ID recipe-type AS "Type"\nFROM #study\nWHERE recipe-type != null',index);assert.deepEqual(result.rows.map(x=>x.values),[['book']])})
test('DQL LIST values, nested tags and repeated clauses are evaluated in written order',()=>{
  assert.deepEqual(runDql('LIST rank\nFROM #study\nSORT rank DESC\nLIMIT 1\nWHERE status = "done"',index).rows,[])
  assert.deepEqual(runDql('LIST rank\nFROM #study\nWHERE status = "done"\nSORT rank DESC\nLIMIT 1',index).rows.map(x=>x.values),[[2]])
})
test('DQL TASK reads source Markdown checkboxes, never impersonates SQLite task DB',()=>{const result=runDql('TASK\nFROM "Projects"\nWHERE !completed',index);assert.equal(result.rows.length,1);assert.equal(result.rows[0].id,task.id);assert.equal(result.rows[0].line,6);assert.equal(result.rows[0].task.managed,false)})
test('DQL comparisons remain typed and keywords inside strings are not rewritten',()=>{
  assert.equal(runDql('LIST\nWHERE rank = "20"',index).rows.length,0)
  const value=doc('Literal.md',{status:'AND OR NOT = FROM SORT LIMIT'})
  assert.equal(runDql('LIST\nWHERE status = "AND OR NOT = FROM SORT LIMIT"',{...index,documents:[value]}).rows.length,1)
  assert.equal(runDql('LIST\nWHERE rank <> 20 AND NOT (status = "done")',index).rows[0].path,'Outside.md')
})
test('DQL fields never traverse prototype, this, formulas or unknown file fields',()=>{for(const field of ['constructor','__proto__','file.ctime','this.file.name','formula.secret','object.nested'])assert.throws(()=>runDql(`TABLE ${field}`,index))})
test('DQL unsupported syntax fails closed even over an empty index',()=>{for(const query of ['TABLE rank\nGROUP BY status','LIST\nFLATTEN tags','CALENDAR file.cday','TABLE rank\nFROM #study OR #other','LIST\nWHERE contains(file.tags,"study")','LIST\nFROM "../outside"','LIST\nFROM "Projects"\nFROM #study','TASK extra','TABLE WITHOUT ID','LIST\nLIMIT -1','LIST\nLIMIT 50001','LIST\nSORT','LIST\nWHERE x + 1'])assert.throws(()=>runDql(query,{...index,documents:[]}))})
test('DQL arbitrary code and excessive nesting are rejected without execution or mutation',()=>{
  globalThis.__dqlExecuted=false
  for(const query of ['LIST\nWHERE globalThis.__dqlExecuted = true','LIST\nWHERE fetch("https://example.com")','LIST\nWHERE (() => true)()','LIST\nWHERE '+ '('.repeat(40)+'true'+')'.repeat(40),'LIST\n'+'WHERE true\n'.repeat(70)])assert.throws(()=>runDql(query,index))
  const before=JSON.stringify(index);runDql('TABLE rank\nSORT rank DESC',index);assert.equal(JSON.stringify(index),before);assert.equal(globalThis.__dqlExecuted,false)
})
test('DQL validates expressions before seeing rows and enforces source row limits',()=>{
  assert.throws(()=>dqlExpression('file.ctime'))
  assert.throws(()=>runDql('LIST',{...index,documents:Array.from({length:50001},(_,i)=>doc(`${i}.md`))}))
  assert.equal(runDql('TABLE file.link, "value, comma" AS "Label"\nLIMIT 0',index).rows.length,0)
})
test('DQL uses empty-container truthiness and null-first scalar sorting, not JS/Base defaults',()=>{
  const values={...index,documents:[doc('Empty.md',{value:[],rank:null}),doc('One.md',{value:['x'],rank:2}),doc('Object.md',{value:{},rank:20})]}
  assert.deepEqual(runDql('LIST\nWHERE value',values).rows.map(row=>row.path),['One.md'])
  assert.deepEqual(runDql('LIST\nWHERE NOT value',values).rows.map(row=>row.path),['Empty.md','Object.md'])
  assert.deepEqual(runDql('LIST\nSORT rank ASC',values).rows.map(row=>row.path),['Empty.md','One.md','Object.md'])
  assert.throws(()=>runDql('LIST\nSORT value',values))
  assert.throws(()=>runDql('LIST\nWHERE file.link = "Empty.md"',values))
})
