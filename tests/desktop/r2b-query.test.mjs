import test from 'node:test'
import assert from 'node:assert/strict'
import {compileExpression,compileFilter,compileBase,noteRow,compareValues} from '../../apps/desktop/src/query-service.ts'
const document=(patch={})=>({path:'Projects/Book.md',title:'A custom heading',properties:{status:'todo',rank:12,flag:false,tags:['book'],'a b':'hello'},tags:['book','parent/child'],links:[{target:'Other',resolvedPath:'Other.md'}],modifiedUnixMs:42,...patch})
const row=noteRow(document())
test('literal comparisons are typed, absent values null, and prototype names blocked',()=>{
  for(const expr of ['rank >= 12','note.rank < 13','flag == false','missing == null','note["a b"] == "hello"','status == "todo"'])assert.equal(compileExpression(expr)(row),true,expr)
  assert.equal(compileExpression('rank == "12"')(row),false);assert.equal(compileExpression('rank > "1"')(row),false)
  for(const expr of ['note.constructor','__proto__','file.unknown','formula.foo','this.file.name'])assert.throws(()=>compileExpression(expr),expr)
})
test('bounded expressions handle precedence, grouping, not and escaped literals',()=>{
  assert.equal(compileExpression('false || rank == 12 && !flag')(row),true)
  assert.equal(compileExpression('(false || true) && status != "done"')(row),true)
  assert.equal(compileExpression('"it\\\'s" == \'it\\\'s\'')(row),true)
  assert.throws(()=>compileExpression('('.repeat(40)+'true'+')'.repeat(40)))
  assert.throws(()=>compileExpression('!'.repeat(40)+'true'))
  assert.throws(()=>compileExpression('true || '.repeat(200)+'true'))
  assert.throws(()=>compileExpression('x'.repeat(5000)))
})
test('file functions and a safe contains/isEmpty subset use metadata only',()=>{
  for(const expr of ['file.hasTag("parent")','file.hasTag("book")','file.inFolder("Projects")','file.hasLink("Other.md")','tags.contains("book")','status.contains("do")','missing.isEmpty()','file.ext == "md"','file.name == "Book"'])assert.equal(compileExpression(expr)(row),true,expr)
  assert.equal(compileExpression('file.inFolder("Project")')(row),false)
  assert.equal(compileExpression('file.inFolder("../")')(row),false)
})
test('declarative filter tree composes global and view conditions without loose coercion',()=>{
  const condition=compileFilter({and:['rank > 3',{or:['status == "done"',{not:['flag']}]}]})
  assert.equal(condition(row),true)
  for(const value of [{not:[]},{not:['a','b']},{and:[],or:[]},[],null,{js:'return true'}])assert.throws(()=>compileFilter(value))
})
test('foreign scripts never execute and unsupported syntax is explicit',()=>{
  globalThis.__r2bPwned=false
  for(const code of ['globalThis.__r2bPwned = true','(()=>{globalThis.__r2bPwned=true})()','window.open("https://x")','constructor("return this")()','fetch("https://x")','rank + 1 > 2','status.toUpperCase()'])assert.throws(()=>compileExpression(code),code)
  assert.equal(globalThis.__r2bPwned,false);delete globalThis.__r2bPwned
})
test('a real base reads order labels multi-sort filters and limit per view',()=>{
  const config={filters:'file.inFolder("Projects")',properties:{'note.rank':{displayName:'Рейтинг'}},views:[{type:'table',name:'Активные',order:['file.name','note.rank'],sort:[{property:'note.rank',direction:'DESC'}],limit:1,filters:'status == "todo"'},{type:'table',name:'Готовые',filters:'status == "done"'}]}
  const before=JSON.stringify(config),result=compileBase(config)
  assert.equal(result.views[0].test(row),true);assert.equal(result.views[1].test(row),false)
  assert.deepEqual(result.views[0].columns,['file.name','note.rank']);assert.equal(result.views[0].limit,1);assert.equal(result.labels['note.rank'],'Рейтинг')
  assert.equal(JSON.stringify(config),before)
})
test('unsupported base views fail closed, keep raw config, and known neighbors still work',()=>{
  const config={formulas:{foo:'fetch("bad")'},future:{preserve:true},views:[{type:'table',order:['formula.foo']},{type:'cards'},{type:'table',filters:'fetch("bad")'},{type:'table',groupBy:{property:'status'}},{type:'table'}]}
  const before=JSON.stringify(config),result=compileBase(config)
  for(const view of result.views.slice(0,4)){assert.ok(view.error);assert.equal(view.test(row),false);assert.equal(view.limit,0)}
  assert.equal(result.views[4].error,null);assert.equal(result.views[4].test(row),true);assert.equal(result.warnings.length,2);assert.equal(JSON.stringify(config),before)
})
test('invalid Base field types and resource limits are rejected',()=>{
  for(const config of [{},{views:[]},{views:Array.from({length:65},()=>({type:'table'}))}])assert.throws(()=>compileBase(config))
  for(const patch of [{order:'title'},{order:[]},{limit:-1},{limit:1.5},{limit:50001},{sort:[{property:'rank',direction:'down'}]},{filters:42}])assert.ok(compileBase({views:[{type:'table',...patch}]}).views[0].error)
})
test('numeric sort is numeric rather than formatted-text order',()=>{
  const collator=new Intl.Collator('ru',{numeric:true});assert.equal(compareValues(2,10,collator),-8);assert.equal(compareValues(null,1,collator),1);assert.ok(compareValues('a2','a10',collator)<0)
})
test('unknown nested objects cannot override formatting or execute code while sorting',()=>{
  const collator=new Intl.Collator('ru');assert.doesNotThrow(()=>compareValues({toString:'not a function'},{constructor:'kept'},collator))
})
