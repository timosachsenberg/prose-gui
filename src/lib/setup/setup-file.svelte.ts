// The setup file of this window (File › Open Setup, Save Setup, Save Setup As): New search's setup and the
// state of the views saved as a `.prose-setup.json` (setup.ts has the format), and opened again: New search gets the
// files, design, database and settings back (`SearchForm.restoreSearch`); if the search the setup ran as is in
// History, Results opens it with its tab, filters, columns and selection. Whatever cannot be brought back (a file moved
// or changed, the database gone, the search not in History) is said in New search's notices, and in a message where
// the setup opens elsewhere: nothing starts by itself (but the download of a UniProt proteome that is no longer on
// disk).

import { toast } from 'svelte-sonner'
import { errorText, forgetSetupFile, openSetupFile, saveSetupFile, useOpenedSetupFile } from '../desktop'
import { APP_VERSION } from '../history/history'
import { history, openFromHistory, refreshHistory } from '../history/history.svelte'
import { resultName } from '../results/display'
import type { Level } from '../results/model'
import { currentResultsView, resultsViewFor, type ResultsView } from '../results/view.svelte'
import { navigate, router } from '../route.svelte'
import { searchForm } from '../search/form.svelte'
import { fetchProteome } from '../search/proteome-download.svelte'
import { onSearchEnded, openResult } from '../search/session.svelte'
import { toolState } from '../search/tool.svelte'
import { DEFAULT_PRESET_ID } from '../settings/presets'
import { formatBytes } from '../format/bytes'
import {
  checkFiles,
  parseSetup,
  proteomeOf,
  serializeSetup,
  SETUP_EXTENSION,
  SETUP_FORMAT,
  SETUP_SCHEMA,
  setupName,
  type SearchSetup,
  type ResultsUi,
  type SetupInput,
} from './setup'

const LEVELS: Level[] = ['proteins', 'peptides', 'psms']

/** "1.2 GB then, 1.3 GB now", in bytes where the rounded sizes look the same. */
function sizes(was: number, now: number): string {
  const [a, b] = formatBytes(was) === formatBytes(now) ? [`${was.toLocaleString('en-GB')} bytes`, `${now.toLocaleString('en-GB')} bytes`] : [formatBytes(was), formatBytes(now)]
  return `${a} then, ${b} now`
}

/** The setup of the window: its name (null: none saved or opened yet) and the search it ran as. */
export const setupFile = $state<{
  name: string | null
  searchId: string | null
  busy: boolean
  confirmNew: boolean
  confirmOpen: { recent?: number } | null
}>({
  name: null,
  searchId: null,
  busy: false,
  /** New Search asks first: the opened setup has changes that are not saved (NewSetupDialog.svelte). */
  confirmNew: false,
  /** Open Setup (`recent`: from Open Recent) asks first, for the same reason. */
  confirmOpen: null,
})

/** The setup as it was when its file was opened or saved last (its search and views aside); null: no setup file. */
// ($state: closing the window asks while the setup has unsaved changes, CloseGuard.svelte, which follows it)
let savedFingerprint = $state<string | null>(null)

/**
 * What of a setup counts as a change to its file: everything but when it was saved, its name, search and views, and
 * the app's and ProSE's versions (no change of the user's; ProSE's is known only once the tool is loaded).
 */
function fingerprint(): string {
  const { savedAt: _t, name: _n, search: _s, ui: _u, appVersion: _a, proseVersion: _p, ...rest } = captureSetup('')
  return JSON.stringify(rest)
}

/** The opened (or saved) setup has changes in New search that are not saved. */
export function hasUnsavedChanges(): boolean {
  return setupFile.name !== null && savedFingerprint !== null && fingerprint() !== savedFingerprint
}

/**
 * File › New Search: a new, unsaved setup. The setup is detached (the next Save asks where; the window's title loses
 * its name) and the form is cleared as Start over clears it (with its Undo). With unsaved changes to an opened
 * setup it asks first (`setupFile.confirmNew`) unless `confirmed`. -> whether the new setup was made.
 */
export function newSetup(confirmed = false): boolean {
  if (!confirmed && hasUnsavedChanges()) {
    setupFile.confirmNew = true
    return false
  }
  setupFile.confirmNew = false
  setupFile.name = null
  setupFile.searchId = null
  savedFingerprint = null
  void forgetSetupFile()
  searchForm.startOver()
  navigate({ view: 'search' })
  return true
}

let started = false

/** A search started here is the setup's search (its id goes into the next save). */
export function initSetupFile(): void {
  if (started) return
  started = true
  onSearchEnded((search) => {
    setupFile.searchId = search.id
  })
}

/** The state of the Results view as a setup keeps it. */
function resultsUiOf(view: ResultsView): ResultsUi {
  const level = (l: Level) => {
    const s = view[l]
    return { filters: $state.snapshot(s.filters), sort: { ...s.sort }, hidden: [...s.hidden], selected: s.selected }
  }
  return {
    tab: view.tab,
    showTechReplicates: view.showTechReplicates,
    panel: view.panel,
    panelTab: view.panelTab,
    panelPeptide: view.panelPeptide,
    psm: view.psm?.row ?? null,
    levels: { proteins: level('proteins'), peptides: level('peptides'), psms: level('psms') },
  }
}

/** The setup of the window now, as a setup file named `name` (setup.ts). */
export function captureSetup(name: string): SearchSetup {
  const form = searchForm
  const route = router.route
  const open = openResult.current
  const view = open ? (currentResultsView()?.result === open ? currentResultsView() : null) : null
  const searchId = setupFile.searchId ?? open?.id ?? null
  const entry = searchId ? history.entries.find((e) => e.id === searchId) : undefined
  const inputs: SetupInput[] = form.files.map((r) => {
    const input: SetupInput = { path: r.file.path, name: r.file.name, size: r.file.size }
    if (r.condition !== undefined) input.condition = r.condition
    for (const f of ['replicate', 'fraction', 'techReplicate'] as const) if (r[f] !== undefined) input[f] = r[f]
    return input
  })
  const db = form.database
  const decoys = form.value('decoys')
  return {
    format: SETUP_FORMAT,
    schema: SETUP_SCHEMA,
    savedAt: new Date().toISOString(),
    appVersion: APP_VERSION,
    proseVersion: toolState.info?.version ?? null,
    name,
    inputs,
    database: db ? { path: db.file.path, name: db.file.name, size: db.file.size, proteome: proteomeOf(db.file.name) } : null,
    contaminants: form.contaminants,
    decoys: typeof decoys === 'string' ? decoys : null,
    presetId: form.presetId,
    parameters: $state.snapshot(form.overrides) as Record<string, unknown>,
    rescore: form.rescore,
    peptdeep: form.peptdeep,
    threads: form.threads,
    search: searchId ? { id: searchId, name: entry?.name ?? open?.stored?.name ?? searchId } : null,
    ui: {
      view: route.view,
      ...(route.view === 'help' && route.page ? { helpPage: route.page } : {}),
      showSettings: form.showSettings,
      advanced: form.advanced,
      results: view ? resultsUiOf(view) : null,
    },
  }
}

/** The name a new setup is offered under: that of its files (as exports name them), or "setup". */
function suggestedName(): string {
  if (setupFile.name) return setupFile.name
  return searchForm.files.length > 0 ? resultName(searchForm.files.map((r) => r.file.name)) : 'search'
}

/**
 * Save Setup (`ask` false: to the setup's file if there is one) or Save Setup As (`ask`: the save dialog). ->
 * where it was saved, or null if cancelled.
 */
export async function saveSetup(ask: boolean): Promise<string | null> {
  const name = suggestedName()
  const saved = await saveSetupFile(`${name}${SETUP_EXTENSION}`, serializeSetup(captureSetup(name)), ask || setupFile.name === null)
  if (!saved) return null
  setupFile.name = setupName(saved.name)
  savedFingerprint = fingerprint()
  toast.success('Setup saved', { description: saved.path })
  return saved.path
}

/**
 * Open Setup (the open dialog) or a recent one (`recent`: its place under File › Open Recent): the setup restored
 * (`restoreSetup`). With unsaved changes to the opened setup it asks first (`setupFile.confirmOpen`) unless
 * `confirmed`. The file becomes the window's setup (Save writes there) only once it reads as one. Resolves with
 * false if cancelled or asking; throws with what is wrong with the file.
 */
export async function openSetup(recent?: number, confirmed = false): Promise<boolean> {
  if (!confirmed && hasUnsavedChanges()) {
    setupFile.confirmOpen = { recent }
    return false
  }
  setupFile.confirmOpen = null
  const chosen = await openSetupFile(recent)
  if (!chosen) return false
  const setup = parseSetup(chosen.text)
  try {
    await restoreSetup(setup, setupName(chosen.path))
  } catch (e) {
    // brought back only in part: no file is the window's setup (the next Save asks where), so that the old one
    // is not written with what New search holds now
    setupFile.name = null
    setupFile.searchId = null
    savedFingerprint = null
    void forgetSetupFile()
    throw e
  }
  // (only now, with the setup brought back, is the file the one Save writes to)
  await useOpenedSetupFile()
  return true
}

/**
 * Brings `setup` back: New search with its files, design, database and settings; then, if its search is in
 * History, Results with the view's state; then the view it was on. Returns the notices about what could not be
 * brought back (also in New search's notices).
 */
export async function restoreSetup(setup: SearchSetup, name = setup.name): Promise<string[]> {
  setupFile.busy = true
  const form = searchForm
  const notes: string[] = []
  try {
    navigate({ view: 'search' })
    // (on top of the preset's: a hand-written file may leave parameters out, which then keep the preset's values, not
    // ProSE's raw defaults such as a PSM FDR of 0; a file the app saved has the preset's parameters among its own)
    const preset = form.presets.find((p) => p.id === setup.presetId) ?? form.presets.find((p) => p.id === DEFAULT_PRESET_ID)
    const parameters = { ...preset?.parameters, ...setup.parameters, ...(setup.decoys ? { decoys: setup.decoys } : {}) }
    const before = form.notices.length
    await form.restoreSearch({
      name,
      inputs: setup.inputs.map((i) => ({
        path: i.path,
        fields: { condition: i.condition, replicate: i.replicate, fraction: i.fraction, techReplicate: i.techReplicate },
      })),
      database: setup.database?.path ?? null,
      databaseName: setup.database?.name ?? 'its FASTA',
      settings: { parameters, rescore: setup.rescore, peptdeep: setup.peptdeep, contaminants: setup.contaminants, presetId: setup.presetId },
    })
    form.threads = setup.threads
    form.advanced = setup.ui.advanced
    form.showSettings = setup.ui.showSettings
    if (setup.presetId && !form.presets.some((p) => p.id === setup.presetId)) {
      notes.push(`The preset of “${name}” (${setup.presetId}) is not on this computer: its settings are shown against the default preset.`)
    }
    // a UniProt proteome no longer on disk is downloaded again (as Protein database › Download does, its progress
    // there); only if that fails is it left to the user
    if (setup.database?.proteome && (form.database === null || form.database.error)) {
      const again = `Choose the database of “${name}” again:`
      try {
        await fetchProteome(setup.database.proteome, /_reviewed\.fasta$/i.test(setup.database.name))
        if (form.database && !form.database.error) form.notices = form.notices.filter((n) => !n.startsWith(again))
        else throw new Error('not readable')
      } catch {
        notes.push(`${setup.database.name} is UniProt proteome ${setup.database.proteome}: download it again under Protein database.`)
      }
    }
    // files that are there but not as they were saved
    const found = [
      ...form.files.map((r) => ({ path: r.file.path, size: r.file.size, error: null })),
      ...(form.database && !form.database.error ? [{ path: form.database.file.path, size: form.database.file.size, error: null }] : []),
    ]
    const expected = [...setup.inputs, ...(setup.database ? [setup.database] : [])].filter((e) => found.some((f) => f.path === e.path))
    const { changed } = checkFiles(expected, found)
    if (changed.length > 0) {
      notes.push(
        `${changed.length === 1 ? 'This file has' : 'These files have'} changed since “${name}” was saved: ` +
          changed.map((c) => `${c.name} (${sizes(c.was, c.now)})`).join(', ') +
          '. Check that ' + (changed.length === 1 ? 'it is' : 'they are') + ' the same data.',
      )
    }
    form.notices.push(...notes)
    const allNotes = [...form.notices.slice(before)]
    setupFile.name = name
    setupFile.searchId = setup.search?.id ?? null
    // (what could not be brought back is no change of the user's)
    savedFingerprint = fingerprint()

    // the search it ran as, with the Results view as it was
    let shownResult = false
    if (setup.search) {
      if (!history.ready) await refreshHistory()
      if (history.entries.some((e) => e.id === setup.search!.id && !e.error)) {
        try {
          await openFromHistory(setup.search.id)
          const open = openResult.current
          if (open?.id === setup.search.id && setup.ui.results) applyResultsUi(resultsViewFor(open), setup.ui.results)
          shownResult = true
        } catch (e) {
          allNotes.push(`The search of “${name}” could not be opened from History (${errorText(e).replace(/\.$/, '')}).`)
          form.notices.push(allNotes.at(-1)!)
        }
      } else {
        const text = `The search of “${name}” (${setup.search.name}) is not in History on this computer: start it again from New search.`
        allNotes.push(text)
        form.notices.push(text)
      }
    }
    const ui = setup.ui
    if (ui.view === 'results') {
      if (!shownResult) navigate({ view: 'search' })
    } else if (ui.view === 'help') navigate({ view: 'help', page: ui.helpPage })
    else navigate({ view: ui.view })
    if (allNotes.length > 0 && router.route.view !== 'search') {
      toast.warning(`“${name}” opened with ${allNotes.length === 1 ? 'a notice' : `${allNotes.length} notices`}`, {
        description: 'New search lists what could not be brought back.',
      })
    }
    return allNotes
  } finally {
    setupFile.busy = false
  }
}

/** The Results view as a setup saved it: rows by index, ignored where the search has no such row. */
export function applyResultsUi(view: ResultsView, ui: ResultsUi): void {
  view.setShowTechReplicates(ui.showTechReplicates)
  const counts: Record<Level, number> = {
    proteins: view.model.tables.proteins.count,
    peptides: view.model.tables.peptides.count,
    psms: view.model.tables.psms.count,
  }
  const inRange = (i: number | null | undefined, l: Level) => (typeof i === 'number' && Number.isInteger(i) && i >= 0 && i < counts[l] ? i : null)
  for (const l of LEVELS) {
    const saved = ui.levels[l]
    const level = view[l]
    const filters = { ...level.filters, ...saved.filters }
    // (a protein or peptide filter of a row the search does not have is dropped)
    if (filters.proteinFilter !== undefined && inRange(filters.proteinFilter, 'proteins') === null) filters.proteinFilter = undefined
    if (filters.peptideFilter !== undefined && inRange(filters.peptideFilter, 'peptides') === null) filters.peptideFilter = undefined
    level.filters = filters
    level.sort = { ...saved.sort }
    level.hidden = [...saved.hidden]
    level.selected = inRange(saved.selected, l)
  }
  const panel = inRange(ui.panel, 'proteins')
  if (panel !== null) {
    view.openProtein(panel)
    view.panelTab = ui.panelTab
    view.panelPeptide = inRange(ui.panelPeptide, 'peptides')
  } else view.panel = null
  const psm = inRange(ui.psm, 'psms')
  view.psm = psm !== null ? { row: psm, origin: { kind: 'psms' } } : null
  if (psm !== null) view.psms.selected = psm
  view.tab = ui.tab
}
