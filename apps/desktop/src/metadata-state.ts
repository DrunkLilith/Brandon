import type { MetadataDelta, VaultIndex, VaultSnapshot } from './types'

export type MetadataStore = { snapshot:VaultSnapshot;index:VaultIndex;generation:number;revision:number }
export function initialMetadata(snapshot:VaultSnapshot):MetadataStore {
  // The snapshot contains the tree, not parsed documents. Request metadata
  // since the cache's creation revision even if the worker already warmed it.
  return { snapshot,index:{documents:[],tags:{},tasks:[]},generation:snapshot.metadataGeneration??0,revision:snapshot.metadataGeneration?1:0 }
}

// Keep unchanged object identities. An edit is an O(N) shallow projection, not
// another parse, deep clone, sort or wire transfer of the whole vault.
function patchRows<T extends {path:string}>(rows:T[],upserts:T[],removed:string[]):T[] {
  if(!upserts.length&&!removed.length)return rows
  const replacements=new Map(upserts.map(row=>[row.path,row])),deleted=new Set(removed),next:T[]=[]
  for(const row of rows){if(deleted.has(row.path))continue;next.push(replacements.get(row.path)??row);replacements.delete(row.path)}
  next.push(...replacements.values());return next
}

export function applyMetadata(store:MetadataStore,delta:MetadataDelta):MetadataStore {
  if(delta.root!==store.snapshot.root||delta.generation<store.generation||delta.generation===store.generation&&delta.revision<store.revision||!delta.reset&&delta.generation!==store.generation)return store
  const before=delta.reset?initialMetadata({...store.snapshot,files:[],entries:[]}):store
  const entries=patchRows(before.snapshot.entries,delta.entries,delta.removedEntries)
  const files=patchRows(before.snapshot.files,delta.entries.filter(entry=>entry.isNote).map(entry=>({path:entry.path,name:entry.name,folder:entry.parent,modifiedUnixMs:entry.modifiedUnixMs,sizeBytes:entry.sizeBytes})),[...delta.removedEntries,...delta.entries.filter(entry=>!entry.isNote).map(entry=>entry.path)])
  const documents=patchRows(before.index.documents,delta.documents,delta.removedDocuments)
  const tags=Object.keys(delta.tags).length?{...before.index.tags}:before.index.tags
  for(const [tag,count] of Object.entries(delta.tags)){if(count)tags[tag]=count;else delete tags[tag]}
  const taskPaths=new Set([...delta.taskPaths,...delta.removedDocuments])
  const tasks=taskPaths.size?[...before.index.tasks.filter(task=>!taskPaths.has(task.path)),...delta.tasks]:before.index.tasks
  return { generation:delta.generation,revision:delta.revision,
    snapshot:entries===store.snapshot.entries&&files===store.snapshot.files?store.snapshot:{...store.snapshot,entries,files,partial:false,metadataRevision:delta.revision,metadataGeneration:delta.generation},
    index:documents===store.index.documents&&tags===store.index.tags&&tasks===store.index.tasks?store.index:{documents,tags,tasks} }
}
