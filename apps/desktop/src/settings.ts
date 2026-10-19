import type { HotkeyOverrides } from './commands'
import type { ViewMode } from './types'

export const SETTINGS_KEY = 'brandon.settings.v1'
export const LEGACY_VISUAL_KEY = 'brandon.visualEditing'
export type DesktopSettings = {
  version: 1
  editorMode: 'visual' | 'source'
  propertiesMode: 'visible' | 'hidden' | 'source'
  defaultViewMode: ViewMode
  readableLineWidth: boolean
  lineNumbers: boolean
  spellcheck: boolean
  previewModifier: 'ctrl' | 'none'
  dailyFolder: string
  dailyFormat: 'YYYY-MM-DD' | 'DD-MM-YYYY' | 'YYYY.MM.DD'
  dailyTemplate: string
  templatesFolder: string
  hotkeys: HotkeyOverrides
}

export function defaultSettings(): DesktopSettings {
  return { version: 1, editorMode: 'visual', propertiesMode:'visible', defaultViewMode: 'edit', readableLineWidth: true, lineNumbers: false, spellcheck: true, previewModifier: 'ctrl',dailyFolder:'Daily',dailyFormat:'YYYY-MM-DD',dailyTemplate:'',templatesFolder:'', hotkeys: {} }
}

export function parseSettings(raw: string | null, legacy: string | null = null): DesktopSettings {
  const settings = defaultSettings()
  try {
    const saved: unknown = raw ? JSON.parse(raw) : null
    if (saved && typeof saved === 'object' && !Array.isArray(saved) && 'version' in saved && saved.version === 1) {
      const value = saved as Record<string, unknown>
      if (value.editorMode === 'visual' || value.editorMode === 'source') settings.editorMode = value.editorMode
      if (value.propertiesMode === 'visible' || value.propertiesMode === 'hidden' || value.propertiesMode === 'source') settings.propertiesMode = value.propertiesMode
      if (value.defaultViewMode === 'edit' || value.defaultViewMode === 'split' || value.defaultViewMode === 'preview') settings.defaultViewMode = value.defaultViewMode
      if (value.previewModifier === 'ctrl' || value.previewModifier === 'none') settings.previewModifier = value.previewModifier
      for(const key of ['dailyFolder','dailyTemplate','templatesFolder'] as const) if(typeof value[key]==='string' && value[key].length<=1024 && (!value[key] || !/^[\/]|[\\:\0]/.test(value[key]) && value[key].split('/').every(part=>part&&part!=='.'&&part!=='..'))) settings[key]=value[key]
      if(value.dailyFormat==='YYYY-MM-DD'||value.dailyFormat==='DD-MM-YYYY'||value.dailyFormat==='YYYY.MM.DD')settings.dailyFormat=value.dailyFormat
      for (const key of ['readableLineWidth', 'lineNumbers', 'spellcheck'] as const) if (typeof value[key] === 'boolean') settings[key] = value[key]
      if (value.hotkeys && typeof value.hotkeys === 'object' && !Array.isArray(value.hotkeys)) {
        for (const [id, binding] of Object.entries(value.hotkeys).slice(0, 256)) {
          if (/^[a-z][a-z0-9-]{0,79}$/.test(id) && (binding === null || typeof binding === 'string' && binding.length <= 80)) settings.hotkeys[id] = binding
        }
      }
    }
  } catch { /* Malformed preferences must not stop a vault from opening. */ }
  // Retain the established preference and migration path for older releases.
  if (legacy === 'visual' || legacy === 'source') settings.editorMode = legacy
  return settings
}

export function readSettings(): DesktopSettings {
  try { return parseSettings(localStorage.getItem(SETTINGS_KEY), localStorage.getItem(LEGACY_VISUAL_KEY)) } catch { return defaultSettings() }
}

export function writeSettings(settings: DesktopSettings) {
  localStorage.setItem(SETTINGS_KEY, JSON.stringify(settings))
  localStorage.setItem(LEGACY_VISUAL_KEY, settings.editorMode)
}
