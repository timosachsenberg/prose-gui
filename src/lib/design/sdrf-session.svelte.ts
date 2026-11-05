// The SDRF loaded into New search: the file as it is (`SdrfTable`; ProSE gets it with the design table's edits,
// `text`), where each of its data files is, and the rows of the design table they became. A data file is looked for
// beside the SDRF, then below a folder the user chose ("Locate folder…", remembered per SDRF), or picked by hand
// ("Locate…"). Found files join the table as if added; the search waits for the others.

import { chooseFile, chooseFolder, errorText, locateDataFiles, openDesignFile, readDesignFile } from '../desktop'
import { brukerFolder, isDesign, isSpectrum } from '../search/files'
import { fileName, searchForm as form } from '../search/form.svelte'
import { parseDesignTsv } from './design-tsv'
import type { DesignInput } from './model'
import { applyDesign } from './paste'
import { parseSdrf } from './sdrf'
import { cell, column, dataRows, noValue, parseSdrfTable, syncDesign, validateSdrf, writeSdrfTable, type SdrfIssue, type SdrfTable } from './sdrf-table'

/**
 * found: the file named; mzml: an mzML of the same stem (the vendor file named is not there, or ProSE cannot read
 * it); vendor: only a vendor file ProSE cannot read (convert it to mzML); remote: not here, but the SDRF has its
 * comment[file uri]; missing: nowhere.
 */
export type FileStatus = 'found' | 'mzml' | 'vendor' | 'remote' | 'missing'

export type SdrfFile = { name: string; status: FileStatus; path: string | null; uri: string }

export const STATUS_TEXT: Record<FileStatus, string> = {
  found: 'Found',
  mzml: 'Found as mzML',
  vendor: 'Vendor file only: convert it to mzML',
  remote: 'Not here; the SDRF has its download address',
  missing: 'Missing',
}

/** The status of the data file `name` from what `locate_data_files` found. */
export function fileStatus(name: string, located: { exact: string | null; mzml: string | null }, uri: string): Omit<SdrfFile, 'name' | 'uri'> {
  if (located.exact && isSpectrum(name)) return { status: 'found', path: located.exact }
  if (located.mzml) return { status: 'mzml', path: located.mzml }
  if (located.exact) return { status: 'vendor', path: located.exact }
  return { status: uri !== '' && !noValue(uri) ? 'remote' : 'missing', path: null }
}

/** The command that converts a vendor file ProSE cannot read into mzML. */
export function convertCommand(path: string): string {
  return /\.raw$/i.test(path) ? `ThermoRawFileParser -i "${path}" -f 2` : `msconvert --mzML --zlib "${path}"`
}

const FOLDERS = 'prose.sdrf.folders'

function rememberedFolder(sdrf: string): string | null {
  try {
    return (JSON.parse(localStorage.getItem(FOLDERS) ?? '{}') as Record<string, string>)[sdrf] ?? null
  } catch {
    return null
  }
}

function rememberFolder(sdrf: string, folder: string) {
  try {
    const all = JSON.parse(localStorage.getItem(FOLDERS) ?? '{}') as Record<string, string>
    all[sdrf] = folder
    localStorage.setItem(FOLDERS, JSON.stringify(all))
  } catch {
    // (a convenience only)
  }
}

class SdrfSession {
  /** The SDRF's path; null: none loaded. */
  path = $state<string | null>(null)
  table = $state.raw<SdrfTable | null>(null)
  issues = $state.raw<SdrfIssue[]>([])
  /** One per distinct comment[data file], in SDRF order. */
  files = $state<SdrfFile[]>([])
  folder = $state<string | null>(null)
  busy = $state(false)
  /** The user chose to search without the files that are not found. */
  skipMissing = $state(false)
  /** The design fields of each data file as loaded, by its name. */
  #loaded = new Map<string, Partial<DesignInput>>()
  /** The SDRF loaded (counted up by load, restore and clear): a look-up of an earlier one changes nothing. */
  #generation = 0
  /** The rows of the design table that got their fields from this SDRF (by path); later look-ups leave them. */
  #seeded = new Set<string>()

  name = $derived(this.path ? fileName(this.path) : '')
  unresolved = $derived(this.files.filter((f) => f.status === 'vendor' || f.status === 'remote' || f.status === 'missing'))

  /** Why the search cannot start (the plan line): data files that are not found. */
  problem = $derived.by(() => {
    if (this.busy) return `Looking for the data files of ${this.name}…`
    const n = this.unresolved.length
    if (n === 0 || this.skipMissing) return null
    const shown = this.unresolved.slice(0, 2).map((f) => f.name).join(', ')
    const more = n > 2 ? ` and ${n - 2} more` : ''
    return `${n} ${n === 1 ? 'data file' : 'data files'} of ${this.name} not found or not readable (${shown}${more}). Locate them, or search without them.`
  })

  /** The SDRF file of a row of the design table (by its path), or undefined for a file added by hand. */
  fileAt(path: string): SdrfFile | undefined {
    return this.files.find((f) => f.path === path)
  }

  /** One entry per distinct data file of `table`. */
  #filesOf(table: SdrfTable): SdrfFile[] {
    const fileCol = column(table, 'comment[data file]')
    const uriCol = column(table, 'comment[file uri]')
    const files: SdrfFile[] = []
    for (const r of dataRows(table)) {
      const name = cell(table, r, fileCol)
      if (name === '' || files.some((f) => f.name === name)) continue
      files.push({ name, status: 'missing', path: null, uri: cell(table, r, uriCol) })
    }
    return files
  }

  async load(path: string, text: string) {
    const table = parseSdrfTable(text)
    this.#generation++
    this.#seeded = new Set()
    this.path = path
    this.table = table
    this.issues = validateSdrf(table)
    this.skipMissing = false
    this.#loaded = new Map(parseSdrf(text).inputs.map((i) => [i.fileName!, i]))
    this.files = this.#filesOf(table)
    const notices = parseSdrf(text).notices.map((n) => `${this.name}: ${n.message}`)
    form.notices.push(...notices)
    this.folder = rememberedFolder(path)
    await this.#resolve()
  }

  /** "Locate folder…": looks for the data files below a folder the user chooses. */
  async locateFolder() {
    if (!this.path) return
    const folder = await chooseFolder(`Where are the data files of ${this.name}?`)
    if (!folder) return
    this.folder = folder
    rememberFolder(this.path, folder)
    await this.#resolve()
  }

  /** "Locate…" of one data file: the user picks it. */
  async locateFile(name: string) {
    const chosen = await chooseFile(`Locate ${name}`)
    if (!chosen) return
    // (a Bruker .d is chosen by its analysis.tdf)
    const path = brukerFolder(chosen)
    const file = this.files.find((f) => f.name === name)
    if (!file) return
    Object.assign(file, { status: isSpectrum(fileName(path)) ? 'found' : 'vendor', path })
    await this.#addFound()
  }

  /** The SDRF as it is, for `restore` (the Undo of a Re-run that replaced it). */
  snapshot() {
    return {
      path: this.path, table: this.table, issues: this.issues, files: $state.snapshot(this.files), folder: this.folder,
      skipMissing: this.skipMissing, loaded: this.#loaded, seeded: this.#seeded,
    }
  }

  /** The SDRF of `snapshot` back, as it was (no look-up of its files again). */
  restore(s: ReturnType<SdrfSession['snapshot']>) {
    this.#generation++
    ;({ path: this.path, table: this.table, issues: this.issues, files: this.files, folder: this.folder, skipMissing: this.skipMissing } = s)
    this.#loaded = s.loaded
    this.#seeded = s.seeded
  }

  /** Forgets the SDRF (its rows stay in the table as files added by hand). */
  clear() {
    this.path = null
    this.table = null
    this.issues = []
    this.files = []
    this.#loaded = new Map()
    this.#generation++
    this.#seeded = new Set()
  }

  async #resolve() {
    if (!this.path) return
    const generation = this.#generation
    const files = this.files
    this.busy = true
    try {
      const located = await locateDataFiles(this.path, files.map((f) => f.name), this.folder)
      // (another SDRF loaded meanwhile, or the data files renamed: this look-up is not theirs)
      if (generation !== this.#generation || files !== this.files) return
      files.forEach((f, i) => Object.assign(f, fileStatus(f.name, located[i], f.uri)))
      await this.#addFound()
    } catch (e) {
      if (generation === this.#generation) form.notices.push(`${this.name}: the data files could not be looked for (${errorText(e)}).`)
    } finally {
      if (generation === this.#generation) this.busy = false
    }
  }

  /**
   * Found files join the table (as `addFiles` would add them), and get the SDRF's design fields, also when they were
   * in the table before the SDRF was loaded; once per SDRF: a row that has them keeps what the user changed since.
   */
  async #addFound() {
    const generation = this.#generation
    const listed = new Set(form.files.map((r) => r.file.path))
    const usable = this.files.filter((f) => f.path && !this.#seeded.has(f.path) && (f.status === 'found' || f.status === 'mzml'))
    if (usable.length === 0) return
    const fresh = usable.filter((f) => !listed.has(f.path!))
    if (fresh.length > 0) await form.addFiles(fresh.map((f) => f.path!))
    if (generation !== this.#generation) return
    for (const f of usable) {
      this.#seeded.add(f.path!)
      const row = form.files.find((r) => r.file.path === f.path)
      const fields = this.#loaded.get(f.name)
      if (row && fields) {
        const { fileName: _n, ...rest } = fields
        form.setField(row.id, 'condition', rest.condition)
        form.setField(row.id, 'replicate', rest.replicate)
        form.setField(row.id, 'fraction', rest.fraction)
        form.setField(row.id, 'techReplicate', rest.techReplicate)
      }
    }
  }

  /** The SDRF with the design table's edits in its own columns (`syncDesign`), as text. */
  text(): string {
    if (!this.table) return ''
    const table = structuredClone(this.table)
    const design = form.design
    syncDesign(
      table,
      this.files.flatMap((f) => {
        const i = form.files.findIndex((r) => r.file.path === f.path)
        if (i < 0 || !design.rows[i]) return []
        const r = form.files[i]
        return [
          {
            fileName: f.name,
            before: this.#loaded.get(f.name) ?? {},
            after: { condition: r.condition, replicate: r.replicate, fraction: r.fraction, techReplicate: r.techReplicate },
            resolved: design.rows[i],
          },
        ]
      }),
    )
    return writeSdrfTable(table)
  }
}

export const sdrfSession = new SdrfSession()

/**
 * "Load design (SDRF, design.tsv)" in the app: the open dialog, then an SDRF (by its header) is loaded with its
 * files (`sdrfSession.load`), a design.tsv gives the fields of the files in the table (matched by name).
 */
export async function openDesign() {
  const chosen = await openDesignFile()
  if (chosen) await loadDesign(chosen.path, chosen.text)
}

/**
 * Files of any kind, from the one "Add files" dialog or dropped on the window: an SDRF or design.tsv is loaded as the
 * design (the first one; the SDRF then adds its own files), everything else goes to the form (spectra, a FASTA).
 */
export async function addAnyFiles(paths: string[]) {
  const designs = paths.filter((p) => isDesign(fileName(p)))
  const rest = paths.filter((p) => !isDesign(fileName(p)))
  if (rest.length > 0) await form.addFiles(rest)
  if (designs.length > 1) form.notices.push(`Only one design is loaded at a time: ${fileName(designs[0])}; ${designs.slice(1).map(fileName).join(', ')} left out.`)
  if (designs.length > 0) {
    try {
      await loadDesign(designs[0], await readDesignFile(designs[0]))
    } catch (e) {
      form.notices.push(`${fileName(designs[0])}: ${String((e as Error)?.message ?? e)}`)
    }
  }
}

/** An SDRF (by its header) or a design.tsv, read from `path`. */
export async function loadDesign(path: string, text: string) {
  const chosen = { path, text }
  const header = chosen.text.replace(/^﻿/, '').split(/\r\n|\n|\r/, 1)[0].toLowerCase()
  if (header.includes('comment[data file]') || header.includes('source name')) {
    await sdrfSession.load(chosen.path, chosen.text)
    return
  }
  const parsed = parseDesignTsv(chosen.text)
  const applied = applyDesign(parsed.inputs, form.files.map((r) => r.file.name))
  form.applyUpdates(applied.rows)
  form.notices.push(...[...parsed.notices.map((n) => n.message), ...applied.notices].map((m) => `${fileName(chosen.path)}: ${m}`))
}
