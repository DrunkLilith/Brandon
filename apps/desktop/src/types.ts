export type VaultFile = {
  path: string
  name: string
  folder: string
  modifiedUnixMs: number
  sizeBytes: number
}

export type VaultEntry = {
  path: string
  name: string
  parent: string
  kind: 'file' | 'folder'
  extension: string | null
  modifiedUnixMs: number
  sizeBytes: number
  isNote: boolean
}

export type VaultSnapshot = {
  root: string
  name: string
  files: VaultFile[]
  entries: VaultEntry[]
  partial?: boolean
  metadataRevision?: number
  metadataGeneration?: number
}

export type MetadataStats = { parsedSources:number;readSources:number;bytesRead:number;scannedEntries:number;resolvedSources:number;fullScans:number;pendingSources:number;failedSources:number;skippedSources:number;documents:number;lastBatchMs:number;lastReconciledUnixMs:number|null }
export type MetadataDelta = { root:string;generation:number;revision:number;reset:boolean;entries:VaultEntry[];removedEntries:string[];documents:DocumentMetadata[];removedDocuments:string[];tags:Record<string,number>;tasks:TaskMetadata[];taskPaths:string[];stats:MetadataStats;error:string|null }

export type NoteDocument = {
  path: string
  content: string
  fingerprint: string
  modifiedUnixMs: number
}

export type SaveOutcome =
  | { status: 'saved'; snapshot: VaultSnapshot; document: NoteDocument }
  | { status: 'conflict'; disk: NoteDocument }

export type VaultMutation = {
  snapshot: VaultSnapshot
  path: string
  linkWarnings: { path: string; reason: 'changed' | 'unavailable' | 'invalidCanvas' | 'limit' }[]
}

export type Heading = { level: number; text: string; slug: string; line: number }

export type DocumentLink = {
  target: string
  display: string | null
  heading: string | null
  block: string | null
  line: number
  kind: 'wiki' | 'markdown' | 'embed'
  resolvedPath: string | null
}

export type DocumentMetadata = {
  path: string
  title: string
  aliases: string[]
  headings: Heading[]
  blocks: { id: string; line: number }[]
  tags: string[]
  links: DocumentLink[]
  tasks: TaskMetadata[]
  properties: Record<string, unknown>
  wordCount: number
  modifiedUnixMs: number
}

export type TaskMetadata = {
  id: string
  managed: boolean
  blockId: string | null
  path: string
  line: number
  title: string
  completed: boolean
  status: 'todo' | 'doing' | 'done'
  project: string | null
  revision: string | null
  taskTags: string[]
  scheduled: string | null
  due: string | null
  durationMinutes: number | null
  priority: 'urgent' | 'high' | 'low' | 'lowest' | null
  tags: string[]
}

export type VaultIndex = {
  documents: DocumentMetadata[]
  tags: Record<string, number>
  tasks: TaskMetadata[]
}

export type TaskIndexMutation = {
  snapshot: VaultSnapshot
  index: VaultIndex
}
export type TaskPatch={completed?:boolean;status?:'todo'|'doing'|'done';scheduled?:string|null;due?:string|null;project?:string|null;priority?:TaskMetadata['priority'];tags?:string[];durationMinutes?:number|null}
export type ProjectTaskCreator=(title:string,project:string|null)=>Promise<string>
export type TaskChange={record:{id:string;notePath:string;title:string;status:TaskMetadata['status'];completed:boolean;scheduled:string|null;due:string|null;project:string|null;priority:TaskMetadata['priority'];tags:string[];durationMinutes:number|null};revision:string;metadataWarning:string|null}

export type SearchHit = {
  path: string
  title: string
  snippet: string
  line: number
  score: number
  kind: 'note' | 'task' | 'canvas' | 'file'
  nodeId: string | null
  revision: string
  contentIndexed: boolean
}

export type SearchResponse = {
  hits: SearchHit[]
  indexedSources: number
  nameOnlySources: number
  failedSources: number
  skippedSources: number
  changedSources: number
  removedSources: number
  elapsedMs: number
  scannedSources: number
  lastReconciledUnixMs: number | null
  syncing: boolean
  pendingSources: number
  staleSources: number
}

export type ViewMode = 'edit' | 'split' | 'preview'
