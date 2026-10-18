import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import { getCurrentWebview } from '@tauri-apps/api/webview'
import { getCurrentWindow } from '@tauri-apps/api/window'
import { open } from '@tauri-apps/plugin-dialog'
import { useCallback, useEffect, useMemo, useRef, useState, type MouseEvent as ReactMouseEvent, type ReactNode } from 'react'
import { BaseView } from './base-view'
import { CanvasView } from './canvas-view'
import { AttachmentDetails } from './canvas-attachments'
import { LocalSearch } from './local-search'
import { CommandPalette, type PaletteAction } from './command-palette'
import { createCommandRegistry, type Command, type CommandRegistry } from './commands'
import { CommandMenu } from './command-menu'
import { MarkdownEditor, type EditorAction, type EditorContext, type MarkdownEditorHandle } from './editor'
import { readSettings, writeSettings } from './settings'
import { SettingsDialog } from './settings-dialog'
import { anchorLine, resolveNoteTarget, type NoteLocation } from './note-navigation'
import { NoteHoverPreview } from './note-hover-preview'
import { FileTree } from './file-tree'
import { GraphView } from './graph-view'
import {
  BrandIcon, CalendarIcon, CanvasIcon, CloseIcon, ColumnsIcon, DatabaseIcon,
  EditIcon, FileIcon, FolderIcon, GraphIcon, MaximizeIcon, MinimizeIcon,
  MoreIcon, PlusIcon, PreviewIcon, RefreshIcon, SaveIcon, SearchIcon,
  SidebarIcon, TemplateIcon, SettingsIcon,
} from './icons'
import { MarkdownPreview } from './markdown-preview'
import { PlannerView } from './planner-view'
import { QuickInsertPalette, type QuickInsert } from './quick-insert-palette'
import { RightSidebar } from './right-sidebar'
import type {
  NoteDocument, SaveOutcome, SearchHit, TaskIndexMutation, TaskMetadata, VaultEntry,
  VaultIndex, VaultMutation, VaultSnapshot, ViewMode,
} from './types'

type Notice = { kind: 'error' | 'success' | 'warning'; message: string; details?: string[] }
type SidebarMode = 'files' | 'search' | 'tags'
type PaletteMode = 'commands' | 'notes' | null
type WorkspaceView = 'note' | 'graph' | 'canvas' | 'base' | 'planner'
type EntryDialog = { kind: 'note' | 'folder' | 'canvas' | 'base' | 'rename' | 'move' | 'recover'; entry?: VaultEntry; value: string; parent: string }
type ContextMenu = { entry: VaultEntry; x: number; y: number }

const GRAPH_TAB = 'brandon://graph'
const PLANNER_TAB = 'brandon://planner'
const EMPTY_TAB_PREFIX = 'brandon://new/'
const HOME_TAB = `${EMPTY_TAB_PREFIX}start`
const UI_ZOOM_STORAGE = 'brandon.uiZoom.v2'
const UI_ZOOM_MIN = .8
const UI_ZOOM_MAX = 1.6
const UI_ZOOM_STEP = .1
const RESIZE_HANDLES = [
  ['north', 'North'], ['north-east', 'NorthEast'], ['east', 'East'], ['south-east', 'SouthEast'],
  ['south', 'South'], ['south-west', 'SouthWest'], ['west', 'West'], ['north-west', 'NorthWest'],
] as const

const appWindow = getCurrentWindow()
const appWebview = getCurrentWebview()

function clampUiZoom(value: number) {
  return Math.round(Math.min(UI_ZOOM_MAX, Math.max(UI_ZOOM_MIN, value)) * 10) / 10
}

function rememberedUiZoom() {
  const stored = Number(window.localStorage.getItem(UI_ZOOM_STORAGE))
  return Number.isFinite(stored) && stored >= UI_ZOOM_MIN && stored <= UI_ZOOM_MAX ? stored : 1
}

function errorMessage(error: unknown) {
  return error instanceof Error ? error.message : String(error)
}

function noteTitle(path: string | null) {
  if (path?.startsWith(EMPTY_TAB_PREFIX)) return 'Новая вкладка'
  if (path === GRAPH_TAB) return 'Граф'
  if (path === PLANNER_TAB) return 'Задачи'
  return path?.split('/').at(-1)?.replace(/\.(md|canvas|base)$/i, '') ?? 'Без названия'
}

function viewForPath(path: string): WorkspaceView {
  if (path === GRAPH_TAB) return 'graph'
  if (path === PLANNER_TAB) return 'planner'
  if (/\.canvas$/i.test(path)) return 'canvas'
  if (/\.base$/i.test(path)) return 'base'
  return 'note'
}

function localDateStamp(date = new Date()) {
  const pad = (value: number) => String(value).padStart(2, '0')
  return `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())}`
}

function expandTemplate(source: string, title: string) {
  const now = new Date()
  return source
    .replaceAll('{{title}}', title)
    .replaceAll('{{date}}', localDateStamp(now))
    .replaceAll('{{time}}', now.toLocaleTimeString('ru-RU', { hour: '2-digit', minute: '2-digit' }))
}

function parentPath(path: string | null) {
  if (!path?.includes('/')) return ''
  return path.slice(0, path.lastIndexOf('/'))
}

export function App() {
  const [settings, setSettings] = useState(readSettings)
  const settingsRef = useRef(settings)
  settingsRef.current = settings
  const [settingsOpen, setSettingsOpen] = useState(false)
  const [editorMenu, setEditorMenu] = useState<EditorContext | null>(null)
  const registryRef = useRef<CommandRegistry | null>(null)
  const [vault, setVault] = useState<VaultSnapshot | null>(null)
  const [index, setIndex] = useState<VaultIndex | null>(null)
  const [note, setNote] = useState<NoteDocument | null>(null)
  const [content, setContent] = useState('')
  const [persistedContent, setPersistedContent] = useState('')
  const [openTabs, setOpenTabs] = useState<string[]>([])
  const [emptyTab, setEmptyTab] = useState<string | null>(HOME_TAB)
  const [history, setHistory] = useState<{ entries: string[]; cursor: number }>({ entries: [], cursor: -1 })
  const [workspaceView, setWorkspaceView] = useState<WorkspaceView>('note')
  const [selectedFolder, setSelectedFolder] = useState('')
  const [viewMode, setViewMode] = useState<ViewMode>(settings.defaultViewMode)
  const [sidebarMode, setSidebarMode] = useState<SidebarMode>('files')
  const [sidebarQuery, setSidebarQuery] = useState('')
  const [searchTarget, setSearchTarget] = useState<{ path: string; line: number; nodeId: string | null; nonce: string } | null>(null)
  const [previewFocus, setPreviewFocus] = useState<{ path: string; line: number; nonce: string } | null>(null)
  const [searchAttachment, setSearchAttachment] = useState<{ path: string; root: string } | null>(null)
  const [sidebarOpen, setSidebarOpen] = useState(true)
  const [rightOpen, setRightOpen] = useState(false)
  const [loading, setLoading] = useState(true)
  const [saving, setSaving] = useState(false)
  const [notice, setNotice] = useState<Notice | null>(null)
  const [dialog, setDialog] = useState<EntryDialog | null>(null)
  const [palette, setPalette] = useState<PaletteMode>(null)
  const [contextMenu, setContextMenu] = useState<ContextMenu | null>(null)
  const [conflict, setConflict] = useState<NoteDocument | null>(null)
  const [missingSource, setMissingSource] = useState<string | null>(null)
  const [discardDraftOpen, setDiscardDraftOpen] = useState(false)
  const [templatePicker, setTemplatePicker] = useState(false)
  const [uiZoom, setUiZoom] = useState(rememberedUiZoom)
  const [busyTaskId, setBusyTaskId] = useState<string | null>(null)
  const visualEditing = settings.editorMode === 'visual'
  const setVisualEditing = useCallback((visual: boolean) => setSettings((current) => ({ ...current, editorMode: visual ? 'visual' : 'source' })), [])
  const [quickInsertOpen, setQuickInsertOpen] = useState(false)
  const searchRef = useRef<HTMLInputElement>(null)
  const dialogRef = useRef<HTMLInputElement>(null)
  const editorRef = useRef<MarkdownEditorHandle>(null)
  const savePromise = useRef<Promise<boolean> | null>(null)
  const navigationRequest = useRef(0)
  const openTabsRef = useRef(openTabs)
  const historyRef = useRef(history)
  const closedTabs = useRef<string[]>([])
  const conflictRef = useRef(conflict)
  const missingSourceRef = useRef(missingSource)
  const noteRef = useRef(note)
  const workspaceViewRef = useRef(workspaceView)
  const contentRef = useRef(content)
  const persistedRef = useRef(persistedContent)
  const activeTab = emptyTab ?? (workspaceView === 'graph' ? GRAPH_TAB : workspaceView === 'planner' ? PLANNER_TAB : note?.path ?? HOME_TAB)
  const activeTabRef = useRef(activeTab)
  const dirty = !['graph', 'planner'].includes(workspaceView) && content !== persistedContent
  const closeGuardActive = dirty || missingSource !== null || saving || conflict !== null
  const hasRecovery = Boolean(missingSource && note && !['graph', 'planner'].includes(workspaceView))
  noteRef.current = note
  workspaceViewRef.current = workspaceView
  contentRef.current = content
  persistedRef.current = persistedContent
  activeTabRef.current = activeTab
  openTabsRef.current = openTabs
  historyRef.current = history
  conflictRef.current = conflict
  missingSourceRef.current = missingSource

  const markMissingSource = useCallback((path: string | null) => {
    missingSourceRef.current = path
    setMissingSource(path)
  }, [])

  const beginRecoveryCopy = useCallback(() => {
    const current = noteRef.current
    if (!current || missingSourceRef.current !== current.path) return
    setDialog({ kind: 'recover', value: `${noteTitle(current.path)} (копия)`, parent: '' })
  }, [])

  const showNotice = useCallback((next: Notice) => {
    setNotice(next)
    if (!next.details) window.setTimeout(() => setNotice((current) => current === next ? null : current), 3000)
  }, [])

  const runCommand = useCallback((id: string) => {
    try { void Promise.resolve(registryRef.current?.execute(id)).catch((error) => showNotice({ kind: 'error', message: errorMessage(error) })) }
    catch (error) { showNotice({ kind: 'error', message: errorMessage(error) }) }
  }, [showNotice])
  const closeEditorMenu = useCallback((restoreFocus = true) => { setEditorMenu(null); if (restoreFocus) window.requestAnimationFrame(() => editorRef.current?.focus()) }, [])
  const openEditorMenu = useCallback((context: EditorContext) => { setContextMenu(null); setEditorMenu(context) }, [])

  const reportLinkWarnings = useCallback((result: VaultMutation) => {
    if (!result.linkWarnings?.length) return
    const reasons = { changed: 'изменён извне', unavailable: 'недоступен', invalidCanvas: 'некорректный Canvas', limit: 'превышен безопасный лимит' }
    showNotice({
      kind: 'warning', message: `Объект перемещён или переименован. Не все ссылки обновлены (${result.linkWarnings.length}). Эти документы не перезаписаны.`,
      details: result.linkWarnings.map((warning) => `${warning.path}: ${reasons[warning.reason]}`),
    })
  }, [showNotice])

  const changeUiZoom = useCallback((change: number | 'reset') => {
    setUiZoom((current) => {
      const next = change === 'reset' ? 1 : clampUiZoom(current + change)
      showNotice({ kind: 'success', message: `Масштаб интерфейса: ${Math.round(next * 100)}%` })
      return next
    })
  }, [showNotice])

  useEffect(() => {
    window.localStorage.setItem(UI_ZOOM_STORAGE, String(uiZoom))
    void appWebview.setZoom(uiZoom).catch((error) => showNotice({ kind: 'error', message: `Не удалось изменить масштаб: ${errorMessage(error)}` }))
  }, [showNotice, uiZoom])

  useEffect(() => {
    try { writeSettings(settings) } catch { showNotice({ kind: 'error', message: 'Настройки применены, но не удалось сохранить их локально.' }) }
  }, [settings, showNotice])

  const loadIndex = useCallback(async () => {
    try {
      setIndex(await invoke<VaultIndex>('get_vault_index'))
    } catch (error) {
      showNotice({ kind: 'error', message: errorMessage(error) })
    }
  }, [showNotice])

  const saveNote = useCallback(async (quiet = false, expectedFingerprint?: string): Promise<boolean> => {
    if (savePromise.current) return savePromise.current
    const current = noteRef.current
    if (!current) return true
    if (missingSourceRef.current && expectedFingerprint === undefined) { beginRecoveryCopy(); return false }
    if (conflictRef.current && expectedFingerprint === undefined) return false
    if (contentRef.current === persistedRef.current && expectedFingerprint === undefined) return true
    const contents = contentRef.current
    setSaving(true)
    const pending = (async () => {
      try {
        const outcome = await invoke<SaveOutcome>(viewForPath(current.path) === 'note' ? 'save_note' : 'save_resource', {
          relativePath: current.path,
          contents,
          expectedFingerprint: expectedFingerprint ?? current.fingerprint,
        })
        if (outcome.status === 'conflict') {
          conflictRef.current = outcome.disk
          setConflict(outcome.disk)
          return false
        }
        setVault(outcome.snapshot)
        // A completed save only acknowledges the submitted text, never later typing.
        if (noteRef.current?.path === current.path) {
          noteRef.current = outcome.document
          persistedRef.current = outcome.document.content
          setNote(outcome.document)
          setPersistedContent(outcome.document.content)
          markMissingSource(null)
        }
        conflictRef.current = null
        setConflict(null)
        if (!quiet) showNotice({ kind: 'success', message: 'Изменения записаны на диск' })
        void loadIndex()
        return true
      } catch (error) {
        if (noteRef.current?.path === current.path) markMissingSource(current.path)
        showNotice({ kind: 'error', message: errorMessage(error) })
        return false
      }
    })()
    savePromise.current = pending
    try {
      return await pending
    } finally {
      savePromise.current = null
      setSaving(false)
    }
  }, [beginRecoveryCopy, loadIndex, markMissingSource, showNotice])

  const flushDocument = useCallback(async () => {
    if (savePromise.current && !await savePromise.current) return false
    if (missingSourceRef.current) { beginRecoveryCopy(); return false }
    while (noteRef.current && contentRef.current !== persistedRef.current) {
      if (!await saveNote(true)) return false
    }
    return !conflictRef.current
  }, [beginRecoveryCopy, saveNote])

  useEffect(() => {
    // A clean window needs no JS close listener. Besides avoiding needless IPC,
    // this leaves ordinary close/reload on Tauri's native path.
    if (!closeGuardActive) return
    let disposed = false, closing = false
    let unlisten: (() => void) | undefined
    const register = async () => {
      const stop = await appWindow.onCloseRequested(async (event) => {
        // Always prevent the API helper's default destroy path. After a safe
        // flush, remove this listener and use the ALREADY granted close command.
        // No destroy permission or new window capability is needed.
        event.preventDefault()
        if (closing) return
        closing = true
        try {
          if (!await flushDocument()) return
          await unlisten?.()
          unlisten = undefined
          await appWindow.close()
        } catch (error) {
          showNotice({ kind: 'error', message: errorMessage(error) })
          if (!disposed && !unlisten) void register().catch((failure) => showNotice({ kind: 'error', message: errorMessage(failure) }))
        } finally { closing = false }
      })
      if (disposed) await stop()
      else unlisten = stop
    }
    void register().catch((error) => showNotice({ kind: 'error', message: errorMessage(error) }))
    const release = () => { disposed = true; void Promise.resolve(unlisten?.()).catch(() => {}) }
    window.addEventListener('beforeunload', release)
    return () => { window.removeEventListener('beforeunload', release); release() }
  }, [closeGuardActive, flushDocument, showNotice])

  const recordVisit = useCallback((path: string, record = true) => {
    activeTabRef.current = path
    if (!record) return
    const current = historyRef.current
    if (current.entries[current.cursor] === path) return
    const entries = [...current.entries.slice(0, current.cursor + 1), path].slice(-100)
    const next = { entries, cursor: entries.length - 1 }
    historyRef.current = next
    setHistory(next)
  }, [])

  const attachTab = useCallback((path: string) => {
    const previous = activeTabRef.current
    setOpenTabs((tabs) => {
      if (tabs.includes(path)) return tabs
      const emptyIndex = tabs.indexOf(previous)
      return previous.startsWith(EMPTY_TAB_PREFIX) && emptyIndex >= 0
        ? tabs.map((item, index) => index === emptyIndex ? path : item)
        : [...tabs, path]
    })
  }, [])

  const applyDocument = useCallback((document: NoteDocument, view: WorkspaceView = 'note', record = true) => {
    if (noteRef.current?.path !== document.path) setViewMode(settingsRef.current.defaultViewMode)
    markMissingSource(null)
    setDiscardDraftOpen(false)
    attachTab(document.path)
    noteRef.current = document
    contentRef.current = document.content
    persistedRef.current = document.content
    workspaceViewRef.current = view
    setNote(document)
    setContent(document.content)
    setPersistedContent(document.content)
    setSelectedFolder(parentPath(document.path))
    setEmptyTab(null)
    setWorkspaceView(view)
    recordVisit(document.path, record)
  }, [attachTab, markMissingSource, recordVisit])

  const clearDocument = useCallback(() => {
    markMissingSource(null)
    setDiscardDraftOpen(false)
    setSearchTarget(null)
    setPreviewFocus(null)
    setSearchAttachment(null)
    noteRef.current = null
    contentRef.current = ''
    persistedRef.current = ''
    workspaceViewRef.current = 'note'
    setNote(null)
    setContent('')
    setPersistedContent('')
    setWorkspaceView('note')
    setEmptyTab(HOME_TAB)
  }, [markMissingSource])

  const activateTab = useCallback(async (path: string, record = true): Promise<boolean> => {
    setSearchTarget(null)
    setPreviewFocus(null)
    setSearchAttachment(null)
    const request = ++navigationRequest.current
    if (activeTabRef.current === path) return true
    if (!await flushDocument() || request !== navigationRequest.current) return false
    try {
      if (path.startsWith(EMPTY_TAB_PREFIX)) {
        clearDocument()
        setEmptyTab(path)
        setOpenTabs((tabs) => tabs.includes(path) ? tabs : [...tabs, path])
      } else if ([GRAPH_TAB, PLANNER_TAB].includes(path)) {
        attachTab(path)
        setEmptyTab(null)
        setWorkspaceView(viewForPath(path))
      } else {
        const view = viewForPath(path)
        const document = await invoke<NoteDocument>(view === 'note' ? 'read_note' : 'read_resource', { relativePath: path })
        if (request !== navigationRequest.current || !await flushDocument() || request !== navigationRequest.current) return false
        applyDocument(document, view, record)
        return true
      }
      setRightOpen(false)
      recordVisit(path, record)
      return true
    } catch (error) {
      showNotice({ kind: 'error', message: errorMessage(error) })
      return false
    }
  }, [applyDocument, attachTab, clearDocument, flushDocument, recordVisit, showNotice])

  const loadNote = useCallback((path: string) => activateTab(path), [activateTab])
  const loadResource = loadNote

  const navigateNote = useCallback(async (path: string, location: NoteLocation = {}) => {
    if (!await loadNote(path) || noteRef.current?.path !== path) return
    if (viewForPath(path) !== 'note') return
    const line = anchorLine(contentRef.current, location)
    if (line === null) { showNotice({ kind: 'warning', message: 'Заметка открыта, но указанный заголовок или блок не найден. Файл не изменён.' }); return }
    const nonce = crypto.randomUUID()
    setPreviewFocus({ path, line, nonce })
    setSearchTarget({ path, line, nodeId: null, nonce })
  }, [loadNote, showNotice])

  const openNoteLink = useCallback((target: string, kind: 'wiki' | 'markdown' = 'wiki') => {
    const current = noteRef.current
    if (!current) return
    const resolved = resolveNoteTarget(target, current.path, index?.documents ?? [], kind)
    if (resolved.status !== 'resolved') {
      const messages = { missing: 'Заметка не найдена. Файл не создан.', ambiguous: 'Несколько заметок подходят под эту ссылку. Укажите путь от папки или корня vault.', invalid: 'Эта ссылка не является безопасным локальным путём заметки.' }
      showNotice({ kind: 'warning', message: messages[resolved.status] }); return
    }
    void navigateNote(resolved.path, resolved)
  }, [index, navigateNote, showNotice])
  const filterTag = useCallback((tag: string) => { setSidebarMode('search'); setSidebarOpen(true); setSidebarQuery(`tag:${tag}`) }, [])
  const openSearchHit = useCallback(async (hit: SearchHit) => {
    if (hit.kind === 'file') { if (vault) setSearchAttachment({ path: hit.path, root: vault.root }); return }
    if (!await flushDocument() || !await loadNote(hit.path) || noteRef.current?.path !== hit.path) return
    setSearchAttachment(null)
    let line = hit.line
    if (hit.revision && noteRef.current.fingerprint !== hit.revision) {
      const relocated = hit.snippet ? contentRef.current.split('\n').findIndex((value) => value.includes(hit.snippet)) : -1
      if (relocated >= 0) line = relocated + 1
      showNotice({ kind: 'success', message: 'Источник изменился после поиска. Открыта текущая версия.' })
    }
    if (!hit.nodeId) setViewMode('edit')
    setSearchTarget({ path: hit.path, line, nodeId: hit.nodeId, nonce: crypto.randomUUID() })
  }, [flushDocument, loadNote, showNotice, vault])
  const openGraph = useCallback(() => { void activateTab(GRAPH_TAB) }, [activateTab])
  const openPlanner = useCallback(() => { void activateTab(PLANNER_TAB) }, [activateTab])
  const newTab = useCallback(() => { void activateTab(`${EMPTY_TAB_PREFIX}${crypto.randomUUID()}`) }, [activateTab])

  const navigateHistory = useCallback(async (direction: number) => {
    const current = historyRef.current
    const cursor = current.cursor + direction
    const path = current.entries[cursor]
    if (!path || !await activateTab(path, false)) return
    const next = { ...current, cursor }
    historyRef.current = next
    setHistory(next)
  }, [activateTab])

  const remapWorkspacePaths = useCallback((oldPath: string, nextPath: string) => {
    const remap = (path: string) => path === oldPath || path.startsWith(`${oldPath}/`) ? `${nextPath}${path.slice(oldPath.length)}` : path
    setOpenTabs((tabs) => tabs.map(remap))
    const next = { ...historyRef.current, entries: historyRef.current.entries.map(remap) }
    historyRef.current = next
    setHistory(next)
    closedTabs.current = closedTabs.current.map(remap)
  }, [])

  const pruneWorkspacePaths = useCallback((entries: VaultEntry[]) => {
    const paths = new Set(entries.map((entry) => entry.path))
    const exists = (path: string) => path === missingSourceRef.current || path.startsWith('brandon://') || paths.has(path)
    setOpenTabs((tabs) => tabs.filter(exists))
    const current = historyRef.current
    const before = current.entries.slice(0, current.cursor + 1).filter(exists)
    const next = { entries: [...before, ...current.entries.slice(current.cursor + 1).filter(exists)], cursor: before.length - 1 }
    historyRef.current = next
    setHistory(next)
    closedTabs.current = closedTabs.current.filter(exists)
  }, [])

  const refreshVault = useCallback(async (external = false) => {
    try {
      const currentAtRequest = noteRef.current
      const snapshot = await invoke<VaultSnapshot>('refresh_vault')
      if (noteRef.current !== currentAtRequest) return // stale refresh must not prune a newly opened/recovered document
      setVault(snapshot)
      const current = noteRef.current
      if (current && !snapshot.entries.some((entry) => entry.path === current.path)) {
        if (contentRef.current !== persistedRef.current || savePromise.current || conflictRef.current || missingSourceRef.current) {
          markMissingSource(current.path)
        } else {
          setOpenTabs((tabs) => tabs.filter((path) => path !== current.path))
          clearDocument()
        }
      } else if (external && current && !missingSourceRef.current && !['graph', 'planner'].includes(workspaceViewRef.current) && contentRef.current === persistedRef.current) {
        const view = viewForPath(current.path)
        const disk = await invoke<NoteDocument>(view === 'note' ? 'read_note' : 'read_resource', { relativePath: current.path })
        if (disk.fingerprint !== current.fingerprint && noteRef.current?.path === current.path && workspaceViewRef.current === view && contentRef.current === persistedRef.current) applyDocument(disk, view, false)
      }
      pruneWorkspacePaths(snapshot.entries)
      await loadIndex()
    } catch (error) {
      showNotice({ kind: 'error', message: errorMessage(error) })
    }
  }, [applyDocument, clearDocument, loadIndex, markMissingSource, pruneWorkspacePaths, showNotice])

  const checkRecoverySource = useCallback(async () => {
    const current = noteRef.current
    if (!current || !missingSourceRef.current) return
    try {
      const disk = await invoke<NoteDocument>(viewForPath(current.path) === 'note' ? 'read_note' : 'read_resource', { relativePath: current.path })
      if (noteRef.current?.path !== current.path) return
      conflictRef.current = disk
      setConflict(disk)
    } catch {
      showNotice({ kind: 'error', message: 'Исходный файл пока недоступен. Черновик остаётся в редакторе; сохраните копию.' })
    }
  }, [showNotice])

  const discardMissingDraft = useCallback(() => {
    const current = noteRef.current
    if (!current || !missingSourceRef.current || savePromise.current) return
    setDiscardDraftOpen(false)
    conflictRef.current = null
    setConflict(null)
    clearDocument()
    setOpenTabs((tabs) => [...tabs.filter((path) => path !== current.path && path !== HOME_TAB), HOME_TAB])
    pruneWorkspacePaths(vault?.entries ?? [])
    recordVisit(HOME_TAB)
  }, [clearDocument, pruneWorkspacePaths, recordVisit, vault])

  const openVault = useCallback(async () => {
    const selected = await open({ directory: true, multiple: false, title: 'Открыть локальный vault' })
    if (!selected || Array.isArray(selected)) return
    if (!await flushDocument()) return
    try {
      const snapshot = await invoke<VaultSnapshot>('open_vault', { path: selected })
      setVault(snapshot)
      setOpenTabs([])
      historyRef.current = { entries: [], cursor: -1 }
      setHistory(historyRef.current)
      closedTabs.current = []
      clearDocument()
      setSidebarQuery('')
      setSidebarMode('files')
      if (snapshot.files[0]) applyDocument(await invoke<NoteDocument>('read_note', { relativePath: snapshot.files[0].path }))
      await loadIndex()
    } catch (error) {
      showNotice({ kind: 'error', message: errorMessage(error) })
    }
  }, [applyDocument, clearDocument, flushDocument, loadIndex, showNotice])

  useEffect(() => {
    if (!dirty || !note || saving || conflict || missingSource) return
    const timeout = window.setTimeout(() => void saveNote(true), 850)
    return () => window.clearTimeout(timeout)
  }, [conflict, content, dirty, missingSource, note, saveNote, saving])

  useEffect(() => {
    void (async () => {
      try {
        const snapshot = await invoke<VaultSnapshot | null>('restore_vault')
        if (snapshot) {
          setVault(snapshot)
          if (snapshot.files[0]) applyDocument(await invoke<NoteDocument>('read_note', { relativePath: snapshot.files[0].path }))
          await loadIndex()
        }
      } catch (error) {
        showNotice({ kind: 'error', message: errorMessage(error) })
      } finally {
        setLoading(false)
      }
    })()
  }, [applyDocument, loadIndex, showNotice])

  useEffect(() => {
    if (!vault) return
    let timeout = 0
    let disposed = false
    let unlisten: (() => void) | undefined
    void listen<string[]>('vault-changed', () => {
      window.clearTimeout(timeout)
      timeout = window.setTimeout(() => { if (!disposed) void refreshVault(true) }, 300)
    }).then((stop) => { unlisten = stop })
    return () => {
      disposed = true
      window.clearTimeout(timeout)
      unlisten?.()
    }
  }, [refreshVault, vault?.root])

  useEffect(() => {
    if (!searchTarget || searchTarget.nodeId || searchTarget.path !== note?.path || workspaceView !== 'note') return
    const frame = window.requestAnimationFrame(() => {
      if (noteRef.current?.path === searchTarget.path) {
        if (viewMode !== 'preview') editorRef.current?.revealLine(searchTarget.line)
        const candidates = Array.from(document.querySelectorAll<HTMLElement>('.preview-pane [data-source-line]'))
        const target = candidates.filter((element) => Number(element.dataset.sourceLine) <= searchTarget.line && Number(element.dataset.sourceEndLine ?? element.dataset.sourceLine) >= searchTarget.line).at(-1)
          ?? candidates.filter((element) => Number(element.dataset.sourceLine) <= searchTarget.line).at(-1)
        if (target) target.scrollIntoView({ block: 'center' })
        setSearchTarget((current) => current?.nonce === searchTarget.nonce ? null : current)
      }
    })
    return () => window.cancelAnimationFrame(frame)
  }, [searchTarget, note?.path, workspaceView, viewMode])

  useEffect(() => {
    if (!previewFocus) return
    const timeout = window.setTimeout(() => setPreviewFocus((current) => current?.nonce === previewFocus.nonce ? null : current), 1800)
    return () => window.clearTimeout(timeout)
  }, [previewFocus])

  useEffect(() => {
    if (!searchAttachment) return
    const close = (event: KeyboardEvent) => { if (event.key === 'Escape') setSearchAttachment(null) }
    window.addEventListener('keydown', close)
    return () => window.removeEventListener('keydown', close)
  }, [searchAttachment])

  useEffect(() => {
    if (dialog) window.setTimeout(() => dialogRef.current?.focus(), 0)
  }, [dialog])

  useEffect(() => {
    const close = () => setContextMenu(null)
    window.addEventListener('click', close)
    return () => window.removeEventListener('click', close)
  }, [])

  const submitDialog = useCallback(async () => {
    if (!dialog || !vault) return
    if (dialog.kind === 'recover') {
      const current = noteRef.current
      if (!current || current.path !== missingSourceRef.current || savePromise.current) return
      const submitted = contentRef.current
      setSaving(true)
      const pending = (async () => {
        try {
          const extension = viewForPath(current.path) === 'note' ? 'md' : current.path.split('.').at(-1)?.toLowerCase()
          const result = await invoke<VaultMutation>('create_resource', { title: dialog.value, folder: dialog.parent || null, extension, contents: submitted })
          const copy = await invoke<NoteDocument>(extension === 'md' ? 'read_note' : 'read_resource', { relativePath: result.path })
          if (noteRef.current?.path !== current.path) return false
          if (copy.content !== submitted) {
            showNotice({ kind: 'error', message: `Файл «${result.path}» изменился сразу после создания. Черновик остаётся здесь; выберите другое название копии.` })
            return false
          }
          const latest = contentRef.current
          setVault(result.snapshot)
          setOpenTabs((tabs) => tabs.filter((path) => path !== current.path))
          conflictRef.current = null
          setConflict(null)
          applyDocument(copy, viewForPath(copy.path))
          // Only submitted text is acknowledged. Later typing remains dirty on
          // the new path and is never discarded by copy completion.
          contentRef.current = latest
          setContent(latest)
          setDialog(null)
          showNotice({ kind: 'success', message: 'Копия сохранена. Исходный файл не перезаписан.' })
          void loadIndex()
          return true
        } catch (error) {
          showNotice({ kind: 'error', message: errorMessage(error) })
          return false
        }
      })()
      savePromise.current = pending
      try { await pending } finally { savePromise.current = null; setSaving(false) }
      return
    }
    if (!await flushDocument()) return
    try {
      if (dialog.kind === 'note') {
        const result = await invoke<VaultMutation>('create_note', { title: dialog.value, folder: dialog.parent || null })
        setVault(result.snapshot)
        applyDocument(await invoke<NoteDocument>('read_note', { relativePath: result.path }))
      } else if (dialog.kind === 'canvas' || dialog.kind === 'base') {
        const extension = dialog.kind
        const contents = dialog.kind === 'canvas'
          ? '{\n  "nodes": [],\n  "edges": []\n}\n'
          : 'filters:\n  and: []\nviews:\n  - type: table\n    name: Таблица\n'
        const result = await invoke<VaultMutation>('create_resource', { title: dialog.value, folder: dialog.parent || null, extension, contents })
        setVault(result.snapshot)
        applyDocument(await invoke<NoteDocument>('read_resource', { relativePath: result.path }), dialog.kind)
      } else if (dialog.kind === 'folder') {
        const result = await invoke<VaultMutation>('create_folder', { name: dialog.value, parent: dialog.parent || null })
        setVault(result.snapshot)
        setSelectedFolder(result.path)
      } else if (dialog.kind === 'rename' && dialog.entry) {
        const oldPath = dialog.entry.path
        const result = await invoke<VaultMutation>('rename_entry', { relativePath: oldPath, newName: dialog.value, updateLinks: true })
        setVault(result.snapshot)
        remapWorkspacePaths(oldPath, result.path)
        reportLinkWarnings(result)
        if (noteRef.current?.path === oldPath || noteRef.current?.path.startsWith(`${oldPath}/`)) {
          const nextPath = `${result.path}${noteRef.current.path.slice(oldPath.length)}`
          const view = viewForPath(nextPath)
          applyDocument(await invoke<NoteDocument>(view === 'note' ? 'read_note' : 'read_resource', { relativePath: nextPath }), view)
        }
      } else if (dialog.kind === 'move' && dialog.entry) {
        const oldPath = dialog.entry.path
        const result = await invoke<VaultMutation>('move_entry', { relativePath: oldPath, destinationFolder: dialog.parent || null, updateLinks: true })
        setVault(result.snapshot)
        remapWorkspacePaths(oldPath, result.path)
        reportLinkWarnings(result)
        if (noteRef.current?.path === oldPath || noteRef.current?.path.startsWith(`${oldPath}/`)) {
          const nextPath = `${result.path}${noteRef.current.path.slice(oldPath.length)}`
          const view = viewForPath(nextPath)
          applyDocument(await invoke<NoteDocument>(view === 'note' ? 'read_note' : 'read_resource', { relativePath: nextPath }), view)
        }
      }
      setDialog(null)
      await loadIndex()
    } catch (error) {
      showNotice({ kind: 'error', message: errorMessage(error) })
    }
  }, [applyDocument, dialog, flushDocument, loadIndex, remapWorkspacePaths, reportLinkWarnings, showNotice, vault])

  const trashEntry = useCallback(async (entry: VaultEntry) => {
    if (!window.confirm(`Переместить «${entry.name}» в локальную корзину vault?`)) return
    try {
      const snapshot = await invoke<VaultSnapshot>('trash_entry', { relativePath: entry.path })
      setVault(snapshot)
      pruneWorkspacePaths(snapshot.entries)
      if (noteRef.current?.path === entry.path || noteRef.current?.path.startsWith(`${entry.path}/`)) clearDocument()
      await loadIndex()
    } catch (error) {
      showNotice({ kind: 'error', message: errorMessage(error) })
    }
  }, [clearDocument, loadIndex, pruneWorkspacePaths, showNotice])

  const closeTab = useCallback(async (path: string) => {
    const tabs = openTabsRef.current
    const position = tabs.indexOf(path)
    if (position < 0) return
    const next = tabs.filter((item) => item !== path)
    if (path === activeTabRef.current) {
      const replacement = next[Math.min(position, next.length - 1)] ?? HOME_TAB
      if (!await activateTab(replacement)) return
    }
    setOpenTabs((current) => current.filter((item) => item !== path && (next.length > 0 || item !== HOME_TAB)))
    closedTabs.current = [...closedTabs.current, path].slice(-20)
  }, [activateTab])

  const reopenTab = useCallback(async () => {
    const path = closedTabs.current.at(-1)
    if (path && await activateTab(path)) closedTabs.current.pop()
  }, [activateTab])

  const cycleTab = useCallback((direction: number) => {
    const tabs = openTabsRef.current
    if (tabs.length < 2) return
    const position = tabs.indexOf(activeTabRef.current)
    void activateTab(tabs[(position + direction + tabs.length) % tabs.length])
  }, [activateTab])

  useEffect(() => {
    document.querySelector('.title-tab.active')?.scrollIntoView({ block: 'nearest', inline: 'nearest' })
  }, [activeTab])

  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (settingsOpen || editorMenu) {
        if (registryRef.current?.forEvent(event) || closeGuardActive && (event.code === 'F5' || (event.ctrlKey || event.metaKey) && event.code === 'KeyR')) event.preventDefault()
        return // modal/menu owns Escape, focus and capture; dirty reload stays guarded
      }
      if (event.key === 'Escape') { setDiscardDraftOpen(false); setDialog(null); setPalette(null); setContextMenu(null); setTemplatePicker(false); setQuickInsertOpen(false); return }
      if (closeGuardActive && (event.code === 'F5' || ((event.ctrlKey || event.metaKey) && event.code === 'KeyR'))) {
        event.preventDefault()
        if (discardDraftOpen || dialog || palette || templatePicker || quickInsertOpen || conflict) return
        if (missingSourceRef.current) beginRecoveryCopy()
        else void saveNote()
        return
      }
      if (discardDraftOpen || dialog || palette || templatePicker || quickInsertOpen || conflict) return
      const command = registryRef.current?.forEvent(event)
      if (!command) return
      if (command.id.startsWith('editor-') && !(event.target instanceof Element && event.target.closest('.codemirror-host .cm-content'))) return
      event.preventDefault(); event.stopPropagation()
      if (!event.repeat) runCommand(command.id)
    }
    window.addEventListener('keydown', onKeyDown, true)
    return () => window.removeEventListener('keydown', onKeyDown, true)
  }, [beginRecoveryCopy, closeGuardActive, conflict, dialog, discardDraftOpen, editorMenu, palette, quickInsertOpen, runCommand, saveNote, settingsOpen, templatePicker])

  const templateFiles = useMemo(() => vault?.files.filter((file) => /(^|\/)(templates?|шаблоны)(\/|$)/i.test(file.path)) ?? [], [vault])

  const insertTemplate = useCallback(async (path: string) => {
    if (!note || workspaceView !== 'note') return
    try {
      const template = await invoke<NoteDocument>('read_note', { relativePath: path })
      const text = expandTemplate(template.content, noteTitle(note.path))
      setTemplatePicker(false)
      if (viewMode !== 'edit') {
        setViewMode('edit')
        window.setTimeout(() => editorRef.current?.insertText(text), 0)
      } else editorRef.current?.insertText(text)
    } catch (error) {
      showNotice({ kind: 'error', message: errorMessage(error) })
    }
  }, [note, showNotice, viewMode, workspaceView])

  const insertQuick = useCallback((item: QuickInsert) => {
    if (!noteRef.current) return
    if (viewMode === 'preview') setViewMode('edit')
    const apply = (attempts = 0) => {
      const editor = editorRef.current
      if (!editor && attempts < 5) {
        window.setTimeout(() => apply(attempts + 1), 24)
        return
      }
      if (item.id === 'inline-code') editor?.insertInlineCode()
      else if (item.block) editor?.insertBlock(item.value, item.cursorOffset)
      else editor?.insertSnippet(item.value, item.cursorOffset)
    }
    window.setTimeout(() => apply(), 0)
  }, [viewMode])

  const createTemplateStarter = useCallback(async () => {
    if (!vault) return
    if (!await flushDocument()) return
    try {
      let snapshot = vault
      let templateFolder = vault.entries.find((entry) => entry.kind === 'folder' && entry.path.toLocaleLowerCase('ru') === 'templates')?.path
      if (!templateFolder) {
        const folder = await invoke<VaultMutation>('create_folder', { name: 'Templates', parent: null })
        snapshot = folder.snapshot
        templateFolder = folder.path
      }
      const result = await invoke<VaultMutation>('create_note', { title: 'Шаблон заметки', folder: templateFolder })
      setVault(result.snapshot ?? snapshot)
      setTemplatePicker(false)
      applyDocument(await invoke<NoteDocument>('read_note', { relativePath: result.path }), 'note')
      showNotice({ kind: 'success', message: 'Создан Templates/Шаблон заметки.md' })
    } catch (error) {
      showNotice({ kind: 'error', message: errorMessage(error) })
    }
  }, [applyDocument, flushDocument, showNotice, vault])

  const openDailyNote = useCallback(async () => {
    if (!vault) return
    if (!await flushDocument()) return
    const title = localDateStamp()
    const relativePath = `Daily/${title}.md`
    const existing = vault.files.find((file) => file.path.toLocaleLowerCase('ru') === relativePath.toLocaleLowerCase('ru'))
    if (existing) { await loadNote(existing.path); return }
    try {
      let dailyFolder = vault.entries.find((entry) => entry.kind === 'folder' && entry.path.toLocaleLowerCase('ru') === 'daily')?.path
      if (!dailyFolder) {
        const folder = await invoke<VaultMutation>('create_folder', { name: 'Daily', parent: null })
        dailyFolder = folder.path
      }
      const result = await invoke<VaultMutation>('create_note', { title, folder: dailyFolder })
      let document = await invoke<NoteDocument>('read_note', { relativePath: result.path })
      const dailyTemplate = templateFiles.find((file) => /(^|\/)(daily|ежедневн(?:ая|ик))\.md$/i.test(file.path))
      if (dailyTemplate) {
        const template = await invoke<NoteDocument>('read_note', { relativePath: dailyTemplate.path })
        const outcome = await invoke<SaveOutcome>('save_note', { relativePath: result.path, contents: expandTemplate(template.content, title), expectedFingerprint: document.fingerprint })
        if (outcome.status === 'saved') document = outcome.document
      }
      setVault(result.snapshot)
      applyDocument(document, 'note')
      await loadIndex()
    } catch (error) {
      showNotice({ kind: 'error', message: errorMessage(error) })
    }
  }, [applyDocument, flushDocument, loadIndex, loadNote, showNotice, templateFiles, vault])

  const createTask = useCallback(async (title: string, scheduled: string | null) => {
    try {
      const result = await invoke<TaskIndexMutation>('create_task', { title, scheduled })
      setVault(result.snapshot)
      setIndex(result.index)
      showNotice({ kind: 'success', message: scheduled ? 'Задача добавлена в календарь' : 'Задача добавлена во входящие' })
    } catch (error) {
      showNotice({ kind: 'error', message: errorMessage(error) })
      throw error
    }
  }, [showNotice])

  const updateTask = useCallback(async (task: TaskMetadata, patch: { completed?: boolean; scheduled?: string | null; due?: string | null }) => {
    if (busyTaskId === task.id) return
    setBusyTaskId(task.id)
    try {
      const result = await invoke<TaskIndexMutation>('update_task', {
        taskId: task.id,
        relativePath: task.path,
        line: task.line,
        blockId: task.blockId,
        completed: patch.completed ?? null,
        scheduled: patch.scheduled ?? null,
        clearScheduled: patch.scheduled === null,
        due: patch.due ?? null,
        clearDue: patch.due === null,
      })
      setVault(result.snapshot)
      setIndex(result.index)
      showNotice({ kind: 'success', message: patch.completed === true ? 'Задача завершена' : patch.due ? 'Дедлайн задан, задача перенесена' : patch.due === null ? 'Отдельный дедлайн снят' : patch.scheduled === null ? 'Дата снята' : patch.scheduled ? 'Задача перепланирована' : 'Задача обновлена' })
    } catch (error) {
      showNotice({ kind: 'error', message: errorMessage(error) })
    } finally {
      setBusyTaskId(null)
    }
  }, [busyTaskId, showNotice])

  const openTask = useCallback(async (task: TaskMetadata) => {
    setVisualEditing(true)
    setViewMode('edit')
    if (!await loadNote(task.path)) return
    window.setTimeout(() => task.managed
      ? editorRef.current?.revealDescription(task.line)
      : editorRef.current?.revealLine(task.line), 60)
  }, [loadNote])

  const beginWindowDrag = useCallback((event: ReactMouseEvent<HTMLElement>) => {
    if (event.button !== 0 || (event.target as HTMLElement).closest('button, input, textarea, select, a, [role="button"], [role="tab"]')) return
    void appWindow.startDragging()
  }, [])

  const toggleWindowMaximize = useCallback((event: ReactMouseEvent<HTMLElement>) => {
    if ((event.target as HTMLElement).closest('button, input, textarea, select, a, [role="button"], [role="tab"]')) return
    void appWindow.toggleMaximize()
  }, [])

  const folders = useMemo(() => vault?.entries.filter((entry) => entry.kind === 'folder') ?? [], [vault])
  const commands = useMemo<Command[]>(() => {
    const hasVault = Boolean(vault), hasNote = Boolean(note && workspaceView === 'note'), editable = hasNote && viewMode !== 'preview'
    return [
      { id: 'palette', label: 'Открыть палитру команд', defaultShortcut: 'Mod+KeyP', run: () => setPalette('commands') },
      { id: 'settings', label: 'Открыть настройки', detail: 'Редактор и горячие клавиши', defaultShortcut: 'Mod+Shift+Comma', run: () => setSettingsOpen(true) },
      { id: 'save', label: 'Сохранить документ', defaultShortcut: 'Mod+KeyS', enabled: Boolean(note && !['graph', 'planner'].includes(workspaceView)), run: () => saveNote() },
      { id: 'new-tab', label: 'Новая вкладка', defaultShortcut: 'Mod+KeyT', enabled: hasVault, run: newTab },
      { id: 'close-tab', label: 'Закрыть текущую вкладку', defaultShortcut: 'Mod+KeyW', enabled: hasVault, run: () => closeTab(activeTabRef.current) },
      { id: 'reopen-tab', label: 'Вернуть закрытую вкладку', defaultShortcut: 'Mod+Shift+KeyT', enabled: hasVault, run: reopenTab },
      { id: 'next-tab', label: 'Следующая вкладка', defaultShortcut: 'Mod+Tab', enabled: hasVault, run: () => cycleTab(1) },
      { id: 'previous-tab', label: 'Предыдущая вкладка', defaultShortcut: 'Mod+Shift+Tab', enabled: hasVault, run: () => cycleTab(-1) },
      { id: 'navigate-back', label: 'Вернуться назад', defaultShortcut: 'Alt+ArrowLeft', enabled: history.cursor > 0, run: () => navigateHistory(-1) },
      { id: 'navigate-forward', label: 'Перейти вперёд', defaultShortcut: 'Alt+ArrowRight', enabled: history.cursor < history.entries.length - 1, run: () => navigateHistory(1) },
      { id: 'new-note', label: 'Новая заметка', detail: selectedFolder || 'Корень vault', defaultShortcut: 'Mod+KeyN', enabled: hasVault, run: () => setDialog({ kind: 'note', value: '', parent: selectedFolder }) },
      { id: 'new-folder', label: 'Новая папка', enabled: hasVault, run: () => setDialog({ kind: 'folder', value: '', parent: selectedFolder }) },
      { id: 'daily-note', label: 'Открыть сегодняшнюю заметку', enabled: hasVault, run: openDailyNote },
      { id: 'insert-template', label: 'Вставить шаблон', detail: 'Templates', enabled: hasNote, run: () => setTemplatePicker(true) },
      { id: 'quick-insert', label: 'Открыть палитру быстрых вставок', defaultShortcut: 'Mod+Slash', enabled: hasNote, run: () => setQuickInsertOpen(true) },
      { id: 'new-canvas', label: 'Создать новый холст', detail: 'JSON Canvas · .canvas', enabled: hasVault, run: () => setDialog({ kind: 'canvas', value: '', parent: selectedFolder }) },
      { id: 'new-base', label: 'Создать новую базу', detail: 'Obsidian Bases · .base', enabled: hasVault, run: () => setDialog({ kind: 'base', value: '', parent: selectedFolder }) },
      { id: 'planner', label: 'Открыть задачи и календарь', defaultShortcut: 'Mod+Shift+KeyA', enabled: hasVault, run: openPlanner },
      { id: 'graph', label: 'Открыть граф связей', defaultShortcut: 'Mod+KeyG', enabled: hasVault, run: openGraph },
      { id: 'open-note', label: 'Быстрый переход к заметке', defaultShortcut: 'Mod+KeyO', enabled: hasVault, run: () => setPalette('notes') },
      { id: 'search', label: 'Поиск по содержимому', defaultShortcut: 'Mod+KeyK', enabled: hasVault, run: () => { setSidebarMode('search'); setSidebarOpen(true); window.setTimeout(() => searchRef.current?.focus(), 0) } },
      { id: 'toggle-right', label: 'Показать или скрыть контекст', defaultShortcut: 'Mod+Comma', enabled: hasNote, run: () => setRightOpen((value) => !value) },
      { id: 'mode-edit', label: 'Режим редактора', enabled: hasNote, run: () => setViewMode('edit') },
      { id: 'mode-split', label: 'Разделённый режим', enabled: hasNote, run: () => setViewMode('split') },
      { id: 'mode-preview', label: 'Режим чтения', enabled: hasNote, run: () => setViewMode('preview') },
      { id: 'mode-read', label: 'Переключить редактор и чтение', defaultShortcut: 'Mod+KeyE', enabled: hasNote, run: () => setViewMode((mode) => mode === 'preview' ? 'edit' : 'preview') },
      { id: 'mode-visual', label: 'Визуальный редактор', enabled: hasNote, run: () => { setVisualEditing(true); setViewMode('edit') } },
      { id: 'mode-source', label: 'Исходный Markdown', enabled: hasNote, run: () => { setVisualEditing(false); setViewMode('edit') } },
      { id: 'zoom-in', label: 'Увеличить интерфейс', defaultShortcut: 'Mod+Equal', run: () => changeUiZoom(UI_ZOOM_STEP) },
      { id: 'zoom-out', label: 'Уменьшить интерфейс', defaultShortcut: 'Mod+Minus', run: () => changeUiZoom(-UI_ZOOM_STEP) },
      { id: 'zoom-reset', label: 'Сбросить масштаб интерфейса', defaultShortcut: 'Mod+Digit0', run: () => changeUiZoom('reset') },
      { id: 'open-vault', label: 'Открыть другой vault', run: openVault },
      ...([
        ['undo', 'Отменить', 'Mod+KeyZ'], ['redo', 'Повторить', 'Mod+Shift+KeyZ'],
        ['copy', 'Копировать'], ['cut', 'Вырезать'], ['paste', 'Вставить'], ['select-all', 'Выделить всё'],
        ['bold', 'Жирный', 'Mod+KeyB'], ['italic', 'Курсив', 'Mod+KeyI'], ['strike', 'Зачёркнутый', 'Mod+Shift+KeyS'],
        ['inline-code', 'Встроенный код', 'Mod+Backquote'], ['insert-link', 'Вставить внутреннюю ссылку'],
      ] as const).map(([action, label, shortcut]) => ({ id: `editor-${action}`, label, defaultShortcut: shortcut, enabled: editable, run: () => editorRef.current?.execute(action as EditorAction) })),
      { id: 'editor-open-link', label: 'Открыть ссылку под курсором', defaultShortcut: 'Mod+Enter', enabled: editable, run: () => { const link = editorRef.current?.currentLink(); if (link) openNoteLink(link.target, link.kind); else showNotice({ kind: 'warning', message: 'Под курсором нет внутренней ссылки.' }) } },
    ]
  }, [vault, note, workspaceView, viewMode, saveNote, newTab, closeTab, reopenTab, cycleTab, history, navigateHistory, selectedFolder, openDailyNote, openPlanner, openGraph, setVisualEditing, changeUiZoom, openVault, openNoteLink, showNotice])
  const registry = useMemo(() => createCommandRegistry(commands, settings.hotkeys), [commands, settings.hotkeys])
  registryRef.current = registry
  const commandActions = useMemo<PaletteAction[]>(() => commands.filter((command) => command.enabled !== false).map((command) => ({ id: command.id, label: command.label, detail: command.detail, shortcut: registry.shortcut(command.id), run: () => runCommand(command.id) })), [commands, registry, runCommand])

  if (loading) return <div className="boot"><BrandIcon /><span>Brandon</span><i /></div>

  return <div className="app-shell">
    {RESIZE_HANDLES.map(([className, direction]) => <div key={direction} className={`window-resize-handle ${className}`} onPointerDown={(event) => { event.preventDefault(); void appWindow.startResizeDragging(direction) }} />)}
    <header className="titlebar" onMouseDown={beginWindowDrag} onDoubleClick={toggleWindowMaximize}>
      <button className="titlebar-pane-toggle" aria-label="Скрыть боковую панель" onClick={() => setSidebarOpen((value) => !value)}><SidebarIcon /></button>
      <nav className="sidebar-top-tabs" aria-label="Разделы боковой панели">
        <button className={sidebarMode === 'files' ? 'active' : ''} aria-label="Файлы" onClick={() => { setSidebarMode('files'); setSidebarOpen(true) }}><FolderIcon /></button>
        <button className={sidebarMode === 'search' ? 'active' : ''} aria-label="Поиск" onClick={() => { setSidebarMode('search'); setSidebarOpen(true); window.setTimeout(() => searchRef.current?.focus(), 0) }}><SearchIcon /></button>
        <button className={`top-hash ${sidebarMode === 'tags' ? 'active' : ''}`} aria-label="Теги" onClick={() => { setSidebarMode('tags'); setSidebarOpen(true) }}>#</button>
      </nav>
      <div className="titlebar-tabs" role="tablist" aria-label="Открытые вкладки">{vault && openTabs.length === 0 && <div className="title-tab title-tab-empty active" role="tab" aria-selected="true"><span>Новая вкладка</span></div>}{openTabs.map((path) => {
        const view = viewForPath(path)
        const icon = path.startsWith(EMPTY_TAB_PREFIX) ? <PlusIcon /> : view === 'graph' ? <GraphIcon /> : view === 'planner' ? <CalendarIcon /> : view === 'canvas' ? <CanvasIcon /> : view === 'base' ? <DatabaseIcon /> : <FileIcon />
        return <div key={path} className={`title-tab ${path === activeTab ? 'active' : ''}`} role="tab" aria-selected={path === activeTab} tabIndex={path === activeTab ? 0 : -1} title={noteTitle(path)} onClick={() => { void activateTab(path) }} onKeyDown={(event) => { if (event.target === event.currentTarget && (event.key === 'Enter' || event.key === ' ')) { event.preventDefault(); void activateTab(path) } }} onAuxClick={(event) => { if (event.button === 1) { event.preventDefault(); void closeTab(path) } }}>{icon}<span>{noteTitle(path)}</span>{path === activeTab && dirty ? <i /> : <span className="tab-clean" />}<button className="tab-close" aria-label={`Закрыть вкладку «${noteTitle(path)}»`} onClick={(event) => { event.stopPropagation(); void closeTab(path) }}><CloseIcon /></button></div>
      })}{vault && <button className="title-new-tab" aria-label="Новая вкладка" title={`Новая вкладка (${registry.shortcut('new-tab')})`} onClick={() => runCommand('new-tab')}><PlusIcon /></button>}</div>
      <div className="window-controls"><button aria-label="Свернуть" onClick={() => void appWindow.minimize()}><MinimizeIcon /></button><button aria-label="Развернуть" onClick={() => void appWindow.toggleMaximize()}><MaximizeIcon /></button><button aria-label="Закрыть" className="close-window" onClick={() => void appWindow.close()}><CloseIcon /></button></div>
    </header>

    <main className={`workspace ${sidebarOpen ? '' : 'sidebar-hidden'}`}>
      <aside className="activity-rail">
        <button className={`rail-button ${workspaceView === 'graph' ? 'active' : ''}`} aria-label="Открыть граф" data-tooltip="Открыть граф" disabled={!vault} onClick={() => runCommand('graph')}><GraphIcon /></button>
        <button className="rail-button" aria-label="Создать новый холст" data-tooltip="Создать новый холст" disabled={!vault} onClick={() => runCommand('new-canvas')}><CanvasIcon /></button>
        <button className={`rail-button ${workspaceView === 'planner' ? 'active' : ''}`} aria-label="Задачи и календарь" data-tooltip="Задачи и календарь" disabled={!vault} onClick={() => runCommand('planner')}><CalendarIcon /></button>
        <button className="rail-button" aria-label="Вставить шаблон" data-tooltip="Вставить шаблон" disabled={!note || workspaceView !== 'note'} onClick={() => runCommand('insert-template')}><TemplateIcon /></button>
        <button className="rail-button" aria-label="Создать новую базу" data-tooltip="Создать новую базу" disabled={!vault} onClick={() => runCommand('new-base')}><DatabaseIcon /></button>
        <span className="rail-spacer" />
        <button className="rail-button" aria-label="Открыть палитру команд" data-tooltip="Открыть палитру команд" onClick={() => runCommand('palette')}><span className="command-glyph">›_</span></button>
        <button className="rail-button" aria-label="Настройки" data-tooltip="Настройки" onClick={() => runCommand('settings')}><SettingsIcon /></button>
        <button className="rail-button" aria-label="Открыть другую папку" onClick={() => runCommand('open-vault')}><FolderIcon /></button>
      </aside>

      <aside className="sidebar">
        {vault && <>
          <SidebarHeading mode={sidebarMode} onRefresh={() => void refreshVault()} onNewNote={() => setDialog({ kind: 'note', value: '', parent: selectedFolder })} onNewFolder={() => setDialog({ kind: 'folder', value: '', parent: selectedFolder })} />
          {sidebarMode === 'search' && <LocalSearch root={vault.root} revision={index} query={sidebarQuery} onQueryChange={setSidebarQuery} inputRef={searchRef} onOpen={(hit) => void openSearchHit(hit)} />}
          {sidebarMode === 'tags' && <div className="sidebar-search"><SearchIcon /><input ref={searchRef} value={sidebarQuery} onChange={(event) => setSidebarQuery(event.target.value)} placeholder="Фильтр тегов…" /><kbd>Ctrl K</kbd></div>}
          {sidebarMode !== 'search' && <>
          <div className="sidebar-content">{sidebarMode === 'files' && <FileTree entries={vault.entries} activePath={[GRAPH_TAB, PLANNER_TAB].includes(activeTab ?? '') ? null : activeTab} query="" onOpen={(path) => void loadNote(path)} onOpenResource={(path) => void loadResource(path)} onSelectFolder={setSelectedFolder} onContextMenu={(entry, x, y) => setContextMenu({ entry, x, y })} />}
            {sidebarMode === 'tags' && <TagBrowser index={index} query={sidebarQuery} onChoose={(tag) => { setSidebarMode('search'); setSidebarQuery(`tag:${tag}`) }} />}
          </div>
          </>}
          <button className="sidebar-vault-footer" onClick={() => void openVault()}><span>{vault.name}</span><MoreIcon /></button>
        </>}
        {!vault && <button className="sidebar-empty-open" onClick={() => void openVault()}><FolderIcon /><span>Открыть vault</span></button>}
      </aside>

      <div className={`main-area ${rightOpen && note && workspaceView === 'note' ? 'right-open' : ''}`}>
        <section className={`content-shell${hasRecovery ? ' has-recovery' : ''}`}>
          <div className="commandbar"><div className="toolbar-start"><button className="history-button" disabled={history.cursor <= 0} aria-label="Назад" title="Назад (Alt ←)" onClick={() => { void navigateHistory(-1) }}>‹</button><button className="history-button" disabled={history.cursor >= history.entries.length - 1} aria-label="Вперёд" title="Вперёд (Alt →)" onClick={() => { void navigateHistory(1) }}>›</button>{vault && <div className="breadcrumbs"><strong>{noteTitle(activeTab)}</strong></div>}</div>
            <div className="commandbar-actions">
              {note && workspaceView === 'note' && <div className="view-switcher"><ModeButton active={viewMode === 'edit'} label="Редактор" onClick={() => runCommand('mode-edit')}><EditIcon /></ModeButton><ModeButton active={viewMode === 'split'} label="Разделить" onClick={() => runCommand('mode-split')}><ColumnsIcon /></ModeButton><ModeButton active={viewMode === 'preview'} label="Чтение" onClick={() => runCommand('mode-preview')}><PreviewIcon /></ModeButton></div>}
              <div className="ui-zoom-control" aria-label="Масштаб интерфейса"><button aria-label="Уменьшить масштаб" disabled={uiZoom <= UI_ZOOM_MIN} onClick={() => changeUiZoom(-UI_ZOOM_STEP)}>−</button><button className="ui-zoom-value" title="Сбросить масштаб" onClick={() => changeUiZoom('reset')}>{Math.round(uiZoom * 100)}%</button><button aria-label="Увеличить масштаб" disabled={uiZoom >= UI_ZOOM_MAX} onClick={() => changeUiZoom(UI_ZOOM_STEP)}>+</button></div>
              {note && !['graph', 'planner'].includes(workspaceView) && <button className="save-button" disabled={(!dirty && !missingSource) || saving} onClick={() => runCommand('save')}><SaveIcon /><span>{saving ? 'Сохраняю' : missingSource ? 'Сохранить копию' : dirty ? 'Сохранить' : 'Сохранено'}</span></button>}
              {note && workspaceView === 'note' && <button className={`icon-button ${rightOpen ? 'active' : ''}`} aria-label="Контекст заметки" onClick={() => runCommand('toggle-right')}><MoreIcon /></button>}
            </div>
          </div>

          {hasRecovery && <div className="source-recovery" role="status">
            <div><strong>Не удалось сохранить исходный файл</strong><p>{missingSource} — черновик остаётся в редакторе, автосохранение остановлено.</p></div>
            <button className="source-recovery-save" disabled={saving} onClick={beginRecoveryCopy}>Сохранить копию</button>
            <button className="source-recovery-check" disabled={saving} onClick={() => void checkRecoverySource()}>Проверить источник</button>
            <button className="source-recovery-discard" disabled={saving} onClick={() => setDiscardDraftOpen(true)}>Отбросить черновик…</button>
          </div>}
          {!vault ? <Welcome onOpen={() => runCommand('open-vault')} /> : workspaceView === 'graph' ? <GraphView index={index} activePath={note?.path ?? null} onOpen={(path) => void loadNote(path)} /> : workspaceView === 'planner' ? <PlannerView tasks={index?.tasks ?? []} busyId={busyTaskId} onOpen={(task) => void openTask(task)} onCreate={createTask} onUpdate={updateTask} /> : !note ? <EmptyVault onCreate={() => runCommand('new-note')} onCreateBase={() => runCommand('new-base')} onCreateCanvas={() => runCommand('new-canvas')} onOpen={() => runCommand('open-note')} /> : workspaceView === 'canvas' ? <CanvasView key={note.path} focusTarget={searchTarget?.path === note.path ? searchTarget : null} onMissingSearchTarget={() => showNotice({ kind: 'error', message: 'Карточка больше не существует. Обновите поиск.' })} value={content} index={index} onChange={setContent} onOpen={(path) => void loadNote(path)} /> : workspaceView === 'base' ? <BaseView index={index} onOpen={(path) => void loadNote(path)} /> : <article className={`document ${settings.readableLineWidth ? '' : 'wide-lines'}`}>
            <div className="writing-toolbar">
              <div className="writing-mode"><span>Редактор</span><div><button className={visualEditing && viewMode !== 'preview' ? 'active' : ''} onClick={() => runCommand('mode-visual')}>Визуально</button><button className={!visualEditing && viewMode !== 'preview' ? 'active' : ''} onClick={() => runCommand('mode-source')}>Markdown</button></div></div>
              <button className="quick-insert-button" onClick={() => runCommand('quick-insert')}><PlusIcon /><span>Быстрая вставка</span><kbd>{registry.shortcut('quick-insert')}</kbd></button>
            </div>
            <div className={`editor-grid mode-${viewMode}`}>
              {viewMode !== 'preview' && <div className="editor-pane"><div className="pane-label">{visualEditing ? 'ВИЗУАЛЬНО' : 'MARKDOWN'}</div><MarkdownEditor key={note.path} ref={editorRef} value={content} notePaths={vault.files.map((file) => file.path)} visual={visualEditing} settings={settings} onContext={openEditorMenu} onOpenLink={openNoteLink} onChange={setContent} /></div>}
              {viewMode !== 'edit' && <div className="preview-pane"><div className="pane-label">ЧТЕНИЕ</div><MarkdownPreview source={content} index={index} onOpen={openNoteLink} onChange={setContent} focusLine={previewFocus?.path === note.path ? previewFocus.line : undefined} /></div>}
            </div>
          </article>}
        </section>
        {rightOpen && note && workspaceView === 'note' && <RightSidebar activePath={note.path} source={content} index={index} onOpen={(path, location) => void navigateNote(path, location)} onNavigate={(location) => void navigateNote(note.path, location)} onTag={filterTag} />}
        {searchAttachment?.root === vault?.root && searchAttachment && <AttachmentDetails key={searchAttachment.path} path={searchAttachment.path} onClose={() => setSearchAttachment(null)} />}
      </div>
    </main>

    {vault && note && workspaceView === 'note' && !settingsOpen && !palette && !editorMenu && <NoteHoverPreview modifier={settings.previewModifier} root={vault.root} path={note.path} source={content} index={index} />}
    {settingsOpen && <SettingsDialog settings={settings} commands={commands} onChange={setSettings} onClose={() => setSettingsOpen(false)} />}
    {editorMenu && <CommandMenu x={editorMenu.x} y={editorMenu.y} registry={registry} ids={['editor-undo', 'editor-redo', null, 'editor-cut', 'editor-copy', 'editor-paste', 'editor-select-all', null, 'editor-bold', 'editor-italic', 'editor-strike', 'editor-inline-code', 'editor-insert-link', 'editor-open-link', null, 'quick-insert']} disabled={[...(!editorMenu.hasSelection ? ['editor-copy', 'editor-cut'] : []), ...(!editorMenu.canUndo ? ['editor-undo'] : []), ...(!editorMenu.canRedo ? ['editor-redo'] : []), ...(!editorMenu.canFormat ? ['editor-bold', 'editor-italic', 'editor-strike', 'editor-inline-code', 'editor-insert-link', 'quick-insert'] : []), ...(!editorMenu.link ? ['editor-open-link'] : [])]} onClose={closeEditorMenu} onExecute={(id) => { setEditorMenu(null); runCommand(id) }} />}
    {dialog && <EntryDialogView dialog={dialog} folders={folders} inputRef={dialogRef} onChange={setDialog} onClose={() => setDialog(null)} onSubmit={() => void submitDialog()} />}
    {discardDraftOpen && <div className="dialog-backdrop"><div className="dialog discard-draft-dialog" role="alertdialog" aria-modal="true" aria-labelledby="discard-draft-title"><h2 id="discard-draft-title">Отбросить черновик?</h2><p>Несохранённый текст будет убран из редактора. Файлы на диске не изменятся.</p><div><button className="secondary" autoFocus onClick={() => setDiscardDraftOpen(false)}>Отмена</button><button className="primary" onClick={discardMissingDraft}>Отбросить черновик</button></div></div></div>}
    {palette && <CommandPalette mode={palette} files={vault?.files ?? []} actions={commandActions} onClose={() => setPalette(null)} onOpen={(path) => void loadNote(path)} />}
    {templatePicker && <TemplatePicker templates={templateFiles} onClose={() => setTemplatePicker(false)} onInsert={(path) => void insertTemplate(path)} onCreate={() => void createTemplateStarter()} />}
    {quickInsertOpen && <QuickInsertPalette onClose={() => setQuickInsertOpen(false)} onInsert={insertQuick} />}
    {contextMenu && <EntryContextMenu menu={contextMenu} onClose={() => setContextMenu(null)} onRename={() => setDialog({ kind: 'rename', entry: contextMenu.entry, value: contextMenu.entry.name.replace(/\.(md|canvas|base)$/i, ''), parent: contextMenu.entry.parent })} onMove={() => setDialog({ kind: 'move', entry: contextMenu.entry, value: '', parent: contextMenu.entry.parent })} onReveal={() => void invoke('reveal_entry', { relativePath: contextMenu.entry.path })} onTrash={() => void trashEntry(contextMenu.entry)} />}
    {conflict && <ConflictDialog disk={conflict} onLoad={() => { applyDocument(conflict, viewForPath(conflict.path)); conflictRef.current = null; setConflict(null) }} onOverwrite={() => void saveNote(false, conflict.fingerprint)} onClose={() => setConflict(null)} />}
    {notice && <div role="status" className={`notice ${notice.kind}`}><span /><div>{notice.message}{notice.details && <details><summary>Какие документы проверить</summary><ul>{notice.details.map((detail, index) => <li key={index}>{detail}</li>)}</ul></details>}</div>{notice.details && <button aria-label="Закрыть уведомление" onClick={() => setNotice(null)}>×</button>}</div>}
  </div>
}

function SidebarHeading({ mode, onRefresh, onNewNote, onNewFolder }: { mode: SidebarMode; onRefresh: () => void; onNewNote: () => void; onNewFolder: () => void }) {
  return <div className="section-heading"><span>{mode === 'files' ? '' : mode === 'search' ? 'Поиск' : 'Теги'}</span><div>{mode === 'files' && <><button title="Новая заметка" onClick={onNewNote}><EditIcon /></button><button title="Новая папка" onClick={onNewFolder}><FolderIcon /></button></>}<button title="Обновить" onClick={onRefresh}><RefreshIcon /></button></div></div>
}

function TagBrowser({ index, query, onChoose }: { index: VaultIndex | null; query: string; onChoose: (tag: string) => void }) {
  const normalized = query.trim().toLocaleLowerCase('ru')
  const tags = Object.entries(index?.tags ?? {}).filter(([tag]) => tag.toLocaleLowerCase('ru').includes(normalized)).sort((left, right) => right[1] - left[1])
  return <div className="tag-browser">{tags.map(([tag, count]) => <button key={tag} onClick={() => onChoose(tag)}><span>#{tag}</span><b>{count}</b></button>)}{!tags.length && <div className="empty-filter">Тегов пока нет</div>}</div>
}

function EntryDialogView({ dialog, folders, inputRef, onChange, onClose, onSubmit }: { dialog: EntryDialog; folders: VaultEntry[]; inputRef: React.RefObject<HTMLInputElement | null>; onChange: (dialog: EntryDialog) => void; onClose: () => void; onSubmit: () => void }) {
  const labels = { note: ['Новая заметка', 'Название заметки'], folder: ['Новая папка', 'Название папки'], canvas: ['Новый холст', 'Название холста'], base: ['Новая база', 'Название базы'], rename: ['Переименовать', 'Новое название'], move: ['Переместить', 'Куда переместить'], recover: ['Сохранить копию черновика', 'Название копии'] } as const
  const needsName = dialog.kind !== 'move'
  const description = dialog.kind === 'rename' || dialog.kind === 'move'
    ? 'Ссылки [[wiki]] и файловые карточки Canvas обновятся автоматически. Если документ недоступен или изменён извне, появится предупреждение.'
    : dialog.kind === 'recover' ? 'Будет создан отдельный файл в vault. Существующие файлы не перезаписываются; при совпадении названия выберите другое.'
    : dialog.kind === 'canvas' ? 'Будет создан переносимый файл JSON Canvas.'
      : dialog.kind === 'base' ? 'Будет создан совместимый файл Obsidian Bases.'
        : 'Результат появится обычным объектом в вашей папке.'
  return <div className="dialog-backdrop" onMouseDown={(event) => { if (event.currentTarget === event.target) onClose() }}>
    <form className="dialog" onSubmit={(event) => { event.preventDefault(); onSubmit() }}>
      <span className="eyebrow">{dialog.entry?.path ?? 'ЛОКАЛЬНЫЙ VAULT'}</span><h2>{labels[dialog.kind][0]}</h2>
      {needsName && <><label>{labels[dialog.kind][1]}</label><input ref={inputRef} value={dialog.value} onChange={(event) => onChange({ ...dialog, value: event.target.value })} placeholder={dialog.kind === 'note' ? 'Без названия' : 'Название'} /></>}
      {dialog.kind !== 'rename' && <><label>Папка</label><select value={dialog.parent} onChange={(event) => onChange({ ...dialog, parent: event.target.value })}><option value="">Корень vault</option>{folders.filter((folder) => dialog.entry?.path !== folder.path && !folder.path.startsWith(`${dialog.entry?.path}/`)).map((folder) => <option key={folder.path} value={folder.path}>{folder.path}</option>)}</select></>}
      <p>{description}</p>
      <div><button type="button" className="secondary" onClick={onClose}>Отмена</button><button type="submit" className="primary" disabled={needsName && !dialog.value.trim()}>{dialog.kind === 'move' ? 'Переместить' : dialog.kind === 'rename' ? 'Сохранить' : 'Создать'}</button></div>
    </form>
  </div>
}

function TemplatePicker({ templates, onClose, onInsert, onCreate }: { templates: VaultSnapshot['files']; onClose: () => void; onInsert: (path: string) => void; onCreate: () => void }) {
  const [query, setQuery] = useState('')
  const filtered = templates.filter((file) => file.path.toLocaleLowerCase('ru').includes(query.trim().toLocaleLowerCase('ru')))
  return <div className="dialog-backdrop" onMouseDown={(event) => { if (event.currentTarget === event.target) onClose() }}><div className="dialog template-dialog"><button type="button" className="template-dialog-close" aria-label="Закрыть" onClick={onClose}><CloseIcon /></button><h2>Вставить шаблон</h2>{templates.length ? <><div className="template-search"><SearchIcon /><input autoFocus value={query} onChange={(event) => setQuery(event.target.value)} placeholder="Найти шаблон…" /></div><div className="template-list">{filtered.map((file) => <button key={file.path} onClick={() => onInsert(file.path)}><TemplateIcon /><span><strong>{noteTitle(file.path)}</strong><small>{file.path}</small></span></button>)}</div></> : <div className="template-empty"><div className="template-empty-copy"><TemplateIcon /><span><strong>Папка шаблонов пуста</strong><p>Markdown-файлы из <code>Templates</code> или <code>Шаблоны</code> появятся здесь.</p></span></div><button className="template-create" onClick={onCreate}>Создать шаблон</button></div>}</div></div>
}

function EntryContextMenu({ menu, onClose, onRename, onMove, onReveal, onTrash }: { menu: ContextMenu; onClose: () => void; onRename: () => void; onMove: () => void; onReveal: () => void; onTrash: () => void }) {
  const run = (action: () => void) => { action(); onClose() }
  return <div className="entry-menu" style={{ left: Math.min(menu.x, window.innerWidth - 210), top: Math.min(menu.y, window.innerHeight - 190) }} onClick={(event) => event.stopPropagation()}><header>{menu.entry.name}</header><button onClick={() => run(onRename)}>Переименовать <kbd>F2</kbd></button><button onClick={() => run(onMove)}>Переместить…</button><button onClick={() => run(onReveal)}>Показать в Проводнике</button><hr /><button className="danger" onClick={() => run(onTrash)}>В корзину</button></div>
}

function ConflictDialog({ disk, onLoad, onOverwrite, onClose }: { disk: NoteDocument; onLoad: () => void; onOverwrite: () => void; onClose: () => void }) {
  return <div className="dialog-backdrop"><div className="dialog conflict-dialog"><span className="eyebrow">ЗАЩИТА ОТ ПОТЕРИ ДАННЫХ</span><h2>Файл изменён снаружи</h2><p>После открытия заметку изменило другое приложение. Brandon не будет молча затирать новую версию на диске.</p><div className="conflict-preview">{disk.content.slice(0, 500)}</div><div><button className="secondary" onClick={onClose}>Решить позже</button><button className="secondary" onClick={onLoad}>Загрузить с диска</button><button className="primary" onClick={onOverwrite}>Перезаписать моей</button></div></div></div>
}

function ModeButton({ active, children, label, onClick }: { active: boolean; children: ReactNode; label: string; onClick: () => void }) {
  return <button className={active ? 'active' : ''} onClick={onClick} title={label}>{children}<span>{label}</span></button>
}

function Welcome({ onOpen }: { onOpen: () => void }) {
  return <div className="welcome"><div className="welcome-mark"><BrandIcon /></div><span className="eyebrow">LOCAL-FIRST KNOWLEDGE</span><h1>Ваши мысли.<br />Ваши файлы.</h1><p>Откройте папку с Markdown. Никакого импорта в закрытую базу: Brandon редактирует оригиналы, видит внешние изменения и защищает их от перезаписи.</p><button className="primary large" onClick={onOpen}><FolderIcon />Открыть vault</button><small>Markdown · wiki-ссылки · локальный индекс</small></div>
}

function EmptyVault({ onCreate, onCreateBase, onCreateCanvas, onOpen }: { onCreate: () => void; onCreateBase: () => void; onCreateCanvas: () => void; onOpen: () => void }) {
  return <div className="empty-workspace"><div className="empty-workspace-actions">
    <button onClick={onCreate}><EditIcon /><span>Новая заметка</span><kbd>Ctrl N</kbd></button>
    <button onClick={onCreateBase}><DatabaseIcon /><span>Создать базу данных</span></button>
    <button onClick={onCreateCanvas}><CanvasIcon /><span>Новый холст</span></button>
    <button onClick={onOpen}><SearchIcon /><span>Перейти к файлу</span><kbd>Ctrl O</kbd></button>
  </div></div>
}
