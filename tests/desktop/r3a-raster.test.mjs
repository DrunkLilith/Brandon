import assert from 'node:assert/strict'
import {test} from 'node:test'
import {canvasRasterLayout,containRaster,isCanvasRaster,changedRasterPaths} from '../../apps/desktop/src/canvas-raster.ts'
test('raster layout gives image and footer finite bounds within normal and tiny nodes',()=>{
  for(const [width,height]of [[300,230],[150,80],[12,12],[1,1]]){const layout=canvasRasterLayout({id:'n',type:'file',file:'Фото.png',x:0,y:0,width,height});for(const r of[layout.image,layout.footer]){assert.ok(r.x>=0&&r.y>=0&&r.width>0&&r.height>0);assert.ok(r.x+r.width<=width+1e-9);assert.ok(r.y+r.height<=height+1e-9)}}
})
test('native/fallback fit uses contain rather than stretching or cropping the original',()=>{
  const bounds={x:8,y:8,width:300,height:200};assert.deepEqual(containRaster(bounds,100,100),{x:58,y:8,width:200,height:200});assert.deepEqual(containRaster(bounds,600,200),{x:8,y:58,width:300,height:100});assert.equal(isCanvasRaster({type:'file',file:'Фото.PNG'}),true);assert.equal(isCanvasRaster({type:'file',file:'unsafe.svg'}),false)
})
test('metadata image invalidation targets only matching files or removed folders, never a sibling vault',()=>{
  const refs=['Attachments/Photo.png','Images/Other.png'];assert.deepEqual(changedRasterPaths('C:/Vault',['attachments/PHOTO.png'],refs),['Attachments/Photo.png']);assert.deepEqual(changedRasterPaths('C:/Vault',['Attachments'],refs),['Attachments/Photo.png']);assert.deepEqual(changedRasterPaths('C:/Vault',['Note.md'],refs),[]);assert.deepEqual(changedRasterPaths('C:/Vault',null,refs),refs)
  assert.deepEqual(changedRasterPaths('\\\\?\\C:\\Vault',['C:\\Vault\\Attachments\\Photo.png'],refs,true),['Attachments/Photo.png']);assert.deepEqual(changedRasterPaths('C:/Vault',['C:/Vault-other/Attachments/Photo.png'],refs,true),[]);assert.deepEqual(changedRasterPaths('C:/Vault',['C:/Vault'],refs,true),[])
})
