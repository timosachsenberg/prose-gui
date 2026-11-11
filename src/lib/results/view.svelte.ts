// The state of the Results view for the open result: the active tab, and per level (proteins,
// peptides, PSMs) one filter state, the sort order, the selected row and the hidden columns. The
// rows of a level are derived from these: the table cells, the sort order and the CSV all read the
// same filter state (one cutoff). Kept outside the view, so that it survives a visit to New search.

import { curationStoreFor } from '../desktop'
import { formatPercent, formatQ } from '../format/fdr'
import { psmFdrOff } from './overview'
import type { PsmOptionState } from '../psm/psm'
import type { PsmMarks } from '../psm/measure'
import type { OpenResult } from '../search/session.svelte'
import { columnDef } from './columns'
import { toCsv, toTsv, toXlsx } from './csv'
import { cleanNote, curationChanged, NONE, readCuration, writeCuration, type CuratedPsm, type Mark } from './curation'
import { levelTotal } from './figures'
import { applyFilters, initialFilters, sortRows, type FilterState } from './filters'
import { buildModel, peptideCounts, type Level, type ResultModel } from './model'
import { columnSpecs, csvColumns, defaultHidden, visibleSpecs } from './table-columns'

export type Tab = 'overview' | 'proteins' | 'peptides' | 'psms' | 'samples' | 'qc'
/** The tabs of the protein panel. */
export type PanelTab = 'coverage' | 'peptides' | 'psms'
export type SortDir = 'asc' | 'desc'
export type SortState = { key: string; dir: SortDir }

/** The PSM open in the PSM view: a row of `tables.psms`. */
export type OpenPsm = { row: number }

/** What the Peptides and PSM tables can be narrowed to: one protein, or (the PSM table) one peptide. */
export type Narrowing = { protein: number } | { peptide: number }

/** The row open in a level's detail pane and its neighbours in the level's table, as sorted and filtered. */
export type Neighbours = { prev: number | null; next: number | null; position: number; count: number }

/** How a level is sorted at first: proteins and peptides by PSMs, PSMs by q-value. */
const defaultSort: Record<Level, SortState> = {
  proteins: { key: 'psms', dir: 'desc' },
  peptides: { key: 'psms', dir: 'desc' },
  psms: { key: 'qValue', dir: 'asc' },
}

/** The sort keys of the curation columns: they read the marks and notes, which change in place. */
const curationKeys = new Set(['curation', 'curationNote', 'acceptedPsms', 'rejectedPsms'])

/** What "Mark shown" last did, for its Undo: the PSM rows and the marks they had before. */
export type BulkCuration = { rows: Uint32Array; before: Uint8Array; mark: Mark }

export class LevelState {
  readonly level: Level
  readonly #view: ResultsView
  filters: FilterState = $state()!
  sort: SortState = $state()!
  /** The selected row (a protein, peptide or PSM index), or null. */
  selected = $state<number | null>(null)
  hidden = $state<string[]>([])

  constructor(view: ResultsView, level: Level) {
    this.#view = view
    this.level = level
    this.filters = initialFilters(view.model, level)
    this.sort = { ...defaultSort[level] }
    this.hidden = defaultHidden(columnSpecs(view.model, level, view.search), view.model)
  }

  /** The rows that pass the filters, in table order. */
  readonly kept = $derived.by(() => {
    // (the curation filter reads the marks, which change in place)
    if (this.level !== 'proteins' && this.filters.curation !== 'any') void this.#view.curationVersion
    return applyFilters(this.#view.model, this.level, this.filters)
  })

  /** The sort that applies: the chosen one, or the default if its column is gone (e.g. a sample column). */
  readonly effectiveSort = $derived.by(() =>
    columnDef(this.#view.model, this.level, this.sort.key) ? this.sort : defaultSort[this.level],
  )

  /** The rows of the table: filtered and sorted. */
  readonly rows = $derived.by(() => {
    if (curationKeys.has(this.effectiveSort.key)) void this.#view.curationVersion
    return sortRows(this.#view.model, this.level, this.kept, this.effectiveSort.key, this.effectiveSort.dir, this.filters)
  })

  /** The peptide counts the cells show (at the peptide cutoff on the Peptides level). */
  readonly counts = $derived.by(() =>
    peptideCounts(this.#view.model, this.level === 'peptides' ? this.filters.maxQ : NaN),
  )

  /** The rows at the search's cutoff, before the user's filters ("n of total"). */
  readonly total = $derived.by(() => levelTotal(this.#view.model, this.level, this.filters.showDecoys))

  /**
   * What the table says without rows: the filters hide everything, or nothing passed the search's cutoff. A search
   * without a target PSM says so on every level, with the likely cause; the protein FDR is named only if it was applied.
   */
  readonly emptyText = $derived.by(() => {
    const what = { proteins: 'proteins', peptides: 'peptides', psms: 'PSMs' }[this.level]
    if (this.total > 0) return `No ${what} match the filters.`
    const m = this.#view.model
    const cutoff = (fdr: string, q: number) => `the search's ${fdr} (${formatPercent(q)}, ${formatQ(q)})`
    // a search without decoys has no FDR: nothing to pass
    if (psmFdrOff(this.#view.result.outcome.summary)) return levelTotal(m, 'psms', false) === 0 ? 'The search found no PSMs.' : `No ${what}.`
    if (levelTotal(m, 'psms', false) === 0) {
      return `No PSMs pass ${cutoff('PSM FDR', m.psmFdr)}. The database may not match the organism of the sample.`
    }
    if (this.level === 'proteins' && this.#view.result.outcome.summary.proteinFdrApplied) {
      return `No proteins pass ${cutoff('protein FDR', m.proteinFdr)}.`
    }
    return `No ${what} pass ${cutoff('PSM FDR', m.psmFdr)}.`
  })

  readonly specs = $derived.by(() => columnSpecs(this.#view.model, this.level, this.#view.search))
  readonly visible = $derived.by(() => visibleSpecs(this.specs, this.hidden))

  /** Sorts by `key`: a new column starts descending for numbers and ascending for text; the same column flips. */
  sortBy(key: string): void {
    if (this.effectiveSort.key === key) {
      this.sort = { key, dir: this.effectiveSort.dir === 'asc' ? 'desc' : 'asc' }
    } else {
      const numeric = columnDef(this.#view.model, this.level, key)?.number !== undefined
      this.sort = { key, dir: numeric ? 'desc' : 'asc' }
    }
  }

  toggleColumn(id: string, show: boolean): void {
    this.hidden = show ? this.hidden.filter((h) => h !== id) : [...this.hidden, id]
  }

  /** The CSV of the view: the visible columns, the filtered and sorted rows (peptides in ProForma). */
  csv(): string {
    return toCsv(this.#view.model, this.level, this.rows, csvColumns(this.visible), this.filters)
  }

  /** The view as an Excel workbook: the columns and rows of `csv`. */
  xlsx(): Uint8Array {
    return toXlsx(this.#view.model, this.level, this.rows, csvColumns(this.visible), this.filters, this.level)
  }

  /** The same as a tab-separated table (`toTsv`). */
  tsv(): string {
    return toTsv(this.#view.model, this.level, this.rows, csvColumns(this.visible), this.filters)
  }
}

/**
 * Where the curation of a result is kept (its search folder's `curation.json`, desktop.ts): `load` resolves with
 * the file's text, or null when there is none; `save` writes it.
 */
export type CurationStore = { load(): Promise<string | null>; save(text: string): Promise<void> }

export class ResultsView {
  readonly result: OpenResult
  tab = $state<Tab>('proteins')
  /** "Show technical replicates separately" (the Columns menu): one count column per measurement. */
  showTechReplicates = $state(false)
  /** A CSV export is being written (the button says "Exporting…"). */
  exporting = $state(false)
  /**
   * The protein open in the panel at the right of the Proteins table (a row of `tables.proteins`), or
   * null. The panel belongs to the Proteins tab; the protein stays open while another tab shows.
   */
  panel = $state<number | null>(null)
  /** The panel's tab: Peptides is the one the mockup opens with. */
  panelTab = $state<PanelTab>('peptides')
  /** The peptide expanded in the panel's Peptides tab (a row of `tables.peptides`), or null. */
  panelPeptide = $state<number | null>(null)
  /** The peptide open in the detail pane beside the Peptides table (a row of `tables.peptides`), or null. */
  peptidePanel = $state<number | null>(null)

  /**
   * The PSM view: the PSM open under the PSM table, or null. "Previous" and "Next" go through the rows of the PSM
   * table as it is sorted and filtered. The PSM stays open while another tab shows.
   */
  psm = $state.raw<OpenPsm | null>(null)
  /** The toggles of the PSM view's spectrum, kept from one PSM to the next. */
  psmOptions = $state<PsmOptionState>({ ions: ['b', 'y'], charges: null, losses: null, precursor: true, bottom: null })
  /**
   * The measurements and labels the user made on the PSM view's spectra ("Measure", "Label"), by spectrum ("<file>
   * <native ID>", as the viewers keep them: the PSMs of one spectrum, its co-isolated precursors, share them): kept for
   * the session, so that another PSM and back finds them again.
   */
  psmMarks = $state<Record<string, PsmMarks>>({})
  /** Saves the spectrum of the open PSM as SVG: set by the PSM view while it shows a spectrum (the Export menu's item). */
  spectrumExporter: (() => void) | null = null

  /**
   * The curation marks per PSM row (curation.ts; the model's `curation` is this array), changed in place by
   * `curate`; `curationVersion` counts the changes, so that what shows the marks reads it.
   */
  readonly curation: Uint8Array
  /** The user's notes by PSM row (the model's `curationNotes`), changed in place by `setNote`. */
  readonly curationNotes = new Map<number, string>()
  curationVersion = $state(0)
  /** The last "Mark shown" (`curateMany`), which `undoCurateMany` takes back; null: none. */
  lastBulk = $state.raw<BulkCuration | null>(null)
  /** Why the marks could not be read or saved, or null. */
  curationError = $state<string | null>(null)
  readonly #store: CurationStore | null
  /** The curated PSMs of the file that are none of this result, written back as they were. */
  #unmatched: CuratedPsm[] = []
  /** The marks were read (or there were none to read): saving before would lose them. */
  #loaded = false
  #saving = false
  #dirty = false

  readonly search: { openSearch: boolean }

  /** The search rescored its PSMs with Percolator (some file did). */
  readonly rescored: boolean

  readonly model: ResultModel = $derived.by(() =>
    buildModel(this.result.outcome.tables, this.result.design, {
      showTechReplicates: this.showTechReplicates,
      psmFdr: this.result.psmFdr,
      proteinFdr: this.result.proteinFdr,
      rescored: this.rescored,
      curation: this.curation,
      curationNotes: this.curationNotes,
    }),
  )

  readonly proteins: LevelState
  readonly peptides: LevelState
  readonly psms: LevelState

  constructor(result: OpenResult, store: CurationStore | null = null) {
    this.result = result
    this.curation = new Uint8Array(result.outcome.tables.psms.count)
    this.#store = store
    const summary = result.outcome.summary
    this.search = { openSearch: summary.openSearch }
    this.rescored = summary.files.some((f) => f.percolator?.applied === true)
    this.proteins = new LevelState(this, 'proteins')
    this.peptides = new LevelState(this, 'peptides')
    this.psms = new LevelState(this, 'psms')
    this.loaded = store ? this.#load(store) : Promise.resolve()
    if (!store) this.#loaded = true
  }

  /** Resolves when the saved marks have been read (at once without a store). */
  readonly loaded: Promise<void>

  async #load(store: CurationStore): Promise<void> {
    try {
      const text = await store.load()
      if (text !== null) {
        const { marks, notes, unmatched } = readCuration(this.model, text)
        // what the user marked meanwhile wins
        for (let i = 0; i < marks.length; i++) if (this.curation[i] === NONE) this.curation[i] = marks[i]
        for (const [i, note] of notes) if (!this.curationNotes.has(i)) this.curationNotes.set(i, note)
        this.#unmatched = unmatched
        this.#changed()
      }
      this.#loaded = true
      if (this.#dirty) void this.#save()
    } catch (e) {
      // (not loaded: the marks of this session are not saved, so that they do not overwrite the file)
      this.curationError = `The curation could not be read: ${String((e as Error)?.message ?? e)}`
    }
  }

  /** Writes the marks; one write at a time, the last marks last. */
  async #save(): Promise<void> {
    this.#dirty = true
    if (!this.#store || !this.#loaded || this.#saving) return
    this.#saving = true
    try {
      while (this.#dirty) {
        this.#dirty = false
        await this.#store.save(writeCuration(this.model, this.curation, this.curationNotes, this.#unmatched))
      }
      this.curationError = null
    } catch (e) {
      this.curationError = `The curation could not be saved: ${String((e as Error)?.message ?? e)}`
    } finally {
      this.#saving = false
    }
  }

  /**
   * Marks PSM `i` (accepted, rejected, or NONE to take the mark away) and saves the marks. When it is the open PSM
   * and the PSM table's filters now hide it (e.g. "Not curated"), the view goes on to the PSM after it (or before).
   */
  curate(i: number, mark: Mark): void {
    if (!this.#isPsm(i) || this.curation[i] === mark) return
    const open = this.psm?.row === i ? this.psmNeighbours : null
    this.curation[i] = mark
    this.#changed()
    void this.#save()
    if (open && this.psm && !this.psms.kept.includes(i)) {
      const target = open.next ?? open.prev
      if (target !== null) {
        this.psms.selected = target
        this.psm = { row: target }
      }
    }
  }

  /** The marks or notes changed: what shows them reads them again. */
  #changed(): void {
    curationChanged()
    this.curationVersion++
  }

  /** Sets the note of PSM `i` (`cleanNote`; empty: no note) and saves it. */
  setNote(i: number, text: string): void {
    if (!this.#isPsm(i)) return
    const note = cleanNote(text)
    if ((this.curationNotes.get(i) ?? '') === note) return
    if (note) this.curationNotes.set(i, note)
    else this.curationNotes.delete(i)
    this.#changed()
    void this.#save()
  }

  /**
   * "Mark shown": gives the PSM rows `rows` (the PSM table as filtered) the mark `mark` (NONE: takes their marks away)
   * and saves once. `undoCurateMany` gives them back the marks they had.
   */
  curateMany(rows: Uint32Array, mark: Mark): void {
    const valid = rows.filter((i) => this.#isPsm(i))
    if (valid.length === 0) return
    const before = Uint8Array.from(valid, (i) => this.curation[i])
    for (const i of valid) this.curation[i] = mark
    this.lastBulk = { rows: valid, before, mark }
    this.#changed()
    void this.#save()
  }

  /** Takes back the last "Mark shown". */
  undoCurateMany(): void {
    const bulk = this.lastBulk
    if (!bulk) return
    bulk.rows.forEach((i, j) => (this.curation[i] = bulk.before[j]))
    this.lastBulk = null
    this.#changed()
    void this.#save()
  }

  /** The PSMs with a mark or a note. */
  curatedCount(): number {
    void this.curationVersion
    let n = 0
    for (let i = 0; i < this.curation.length; i++) if (this.curation[i] !== NONE || this.curationNotes.has(i)) n++
    return n
  }

  /** The PSM rows with a mark or a note, in the PSM table's order (whatever its filters). */
  curatedRows(): Uint32Array {
    void this.curationVersion
    const rows: number[] = []
    for (let i = 0; i < this.curation.length; i++) if (this.curation[i] !== NONE || this.curationNotes.has(i)) rows.push(i)
    const sort = this.psms.effectiveSort
    return sortRows(this.model, 'psms', Uint32Array.from(rows), sort.key, sort.dir, this.psms.filters)
  }

  /**
   * Splits or merges the technical replicates' count columns. A level sorted by a sample column stays
   * sorted by that sample: by its first measurement when split, by the whole sample when merged.
   */
  setShowTechReplicates(on: boolean): void {
    if (on === this.showTechReplicates) return
    const before = this.model.samples
    this.showTechReplicates = on
    const after = this.model.samples
    for (const level of [this.proteins, this.peptides, this.psms]) {
      const match = /^sample:(\d+)$/.exec(level.sort.key)
      const old = match ? before[Number(match[1])] : undefined
      if (!old) continue
      const next = after.find(
        (s) =>
          s.designSample === old.designSample &&
          (s.techReplicate === old.techReplicate || s.techReplicate === null || old.techReplicate === null),
      )
      if (next) level.sort = { key: `sample:${next.index}`, dir: level.sort.dir }
    }
  }

  /**
   * Opens the panel of protein `p` (a row of the Proteins table) and selects its row. What is no
   * protein (a peptide below the protein FDR has none, an index out of range) opens nothing. Tables that are
   * narrowed to a protein follow to this one (`#follow`).
   */
  openProtein(p: number): void {
    if (!this.#isProtein(p)) return
    if (p !== this.panel) this.panelPeptide = null
    this.proteins.selected = p
    this.panel = p
    this.#follow({ protein: p })
  }

  /** Closes the panel; the protein's row stays selected. */
  closeProtein(): void {
    this.panel = null
  }

  /**
   * Opens the detail of peptide `k` (a row of the Peptides table) beside the table and selects its row. A PSM table
   * that is narrowed to a peptide follows to this one (`#follow`).
   */
  openPeptide(k: number): void {
    if (!this.#isPeptide(k)) return
    this.peptides.selected = k
    this.peptidePanel = k
    this.#follow({ peptide: k })
  }

  /** Closes the peptide's detail; its row stays selected. */
  closePeptide(): void {
    this.peptidePanel = null
  }

  /**
   * The narrowing of the tables below a level: the Peptides and PSM tables show one protein only (`{ protein }`),
   * or the PSM table one peptide (`{ peptide }`, a row of `tables.peptides`). Both tables share the protein: a chip
   * in their filter row ("Protein: <accession>", "Peptide: <ProForma>") says so and takes it away (`widen`). The
   * contaminants or decoys show if the protein or peptide is one. With `reset` the tables start from the search's
   * cutoff, without text, charge and range filters (the user asked for these rows); without it only the narrowing
   * changes (it follows the selection). A peptide keeps the protein if it is one of that protein's. What the
   * narrowing hides closes: the open PSM, the open peptide.
   */
  #narrow(what: Narrowing, reset: boolean): void {
    const m = this.model
    const clear = (f: FilterState): FilterState => (reset ? { ...f, text: '', maxQ: m.psmFdr, charges: undefined, ranges: undefined } : { ...f })
    if ('protein' in what) {
      const p = what.protein
      const shown = (f: FilterState): FilterState => ({
        ...clear(f),
        proteinFilter: p,
        hideContaminants: m.protein.contaminant[p] ? false : f.hideContaminants,
        showDecoys: m.tables.proteins.decoy[p] ? true : f.showDecoys,
      })
      this.peptides.filters = shown(this.peptides.filters)
      this.psms.filters = { ...shown(this.psms.filters), peptideFilter: undefined }
    } else {
      const k = what.peptide
      const f = this.psms.filters
      const protein = f.proteinFilter !== undefined && m.peptide.proteins[k].includes(f.proteinFilter) ? f.proteinFilter : undefined
      this.psms.filters = {
        ...clear(f),
        proteinFilter: protein,
        peptideFilter: k,
        hideContaminants: m.peptide.contaminant[k] ? false : f.hideContaminants,
        showDecoys: m.tables.peptides.decoy[k] ? true : f.showDecoys,
      }
      // (the two tables share the protein)
      if (protein === undefined && this.peptides.filters.proteinFilter !== undefined) this.peptides.filters = { ...this.peptides.filters, proteinFilter: undefined }
    }
    if (this.psm && !this.psms.kept.includes(this.psm.row)) this.psm = null
    if (this.peptidePanel !== null && !this.peptides.kept.includes(this.peptidePanel)) this.peptidePanel = null
  }

  /**
   * Tables that are narrowed follow the row that opens above them: to another protein once they are narrowed to one
   * (the peptide of the old protein goes), the PSM table to another peptide once it is narrowed to one. Tables that
   * are not narrowed stay as they are: narrowing is asked for (`narrowTo`).
   */
  #follow(what: Narrowing): void {
    if ('protein' in what) {
      const [a, b] = [this.peptides.filters.proteinFilter, this.psms.filters.proteinFilter]
      if (a === undefined && b === undefined) return
      if (a === what.protein && b === what.protein) return
    } else if (this.psms.filters.peptideFilter === undefined || this.psms.filters.peptideFilter === what.peptide) return
    this.#narrow(what, false)
  }

  /**
   * The level below, for one row only: the Peptides tab (`tab`) or the PSMs tab with the peptides and PSMs of one
   * `protein`, or the PSMs tab with the PSMs of one `peptide` (see `#narrow`: from the search's cutoff, without the
   * tables' other filters). What is no protein or peptide changes nothing.
   */
  narrowTo(what: Narrowing, tab: 'peptides' | 'psms' = 'psms'): void {
    if ('protein' in what ? !this.#isProtein(what.protein) : !this.#isPeptide(what.peptide)) return
    this.#narrow(what, true)
    this.tab = 'peptide' in what ? 'psms' : tab
  }

  /** "Open in PSM table": the PSMs of one protein or peptide in the PSMs tab (`narrowTo`). */
  filterBy(what: Narrowing): void {
    this.narrowTo(what, 'psms')
  }

  /** Takes a narrowing away (its chip): the protein from the Peptides and PSM tables, or the peptide from the PSM table. */
  widen(which: 'protein' | 'peptide'): void {
    if (which === 'peptide') {
      if (this.psms.filters.peptideFilter !== undefined) this.psms.filters = { ...this.psms.filters, peptideFilter: undefined }
      return
    }
    for (const level of [this.peptides, this.psms]) {
      if (level.filters.proteinFilter !== undefined) level.filters = { ...level.filters, proteinFilter: undefined }
    }
  }

  /** The protein a peptide is shown with: the one the tables are narrowed to if it is its, else its first; null: none. */
  proteinOf(k: number): number | null {
    const refs = this.model.peptide.proteins[k]
    if (!refs || refs.length === 0) return null
    const narrowed = this.psms.filters.proteinFilter ?? this.peptides.filters.proteinFilter
    return narrowed !== undefined && refs.includes(narrowed) ? narrowed : refs[0]
  }

  /**
   * Opens PSM `i` (a row of `tables.psms`) in the PSM view, from the PSM table: the PSMs tab with the
   * PSM selected. A filter that hides it is lifted (text, protein and peptide filters, the cutoff,
   * contaminants or decoys as far as it is one), so that it is a row of the table above the view.
   */
  openPsm(i: number): void {
    if (!this.#isPsm(i)) return
    const m = this.model
    const ps = m.tables.psms
    const level = this.psms
    if (!level.kept.includes(i)) {
      const f = level.filters
      // (the two tables share the protein)
      if (f.proteinFilter !== undefined) this.widen('protein')
      level.filters = {
        ...f,
        text: '',
        maxQ: m.psmFdr,
        proteinFilter: undefined,
        peptideFilter: undefined,
        charges: undefined,
        ranges: undefined,
        hideContaminants: m.peptide.contaminant[ps.peptide[i]] ? false : f.hideContaminants,
        showDecoys: ps.decoy[i] ? true : f.showDecoys,
      }
    }
    level.selected = i
    this.tab = 'psms'
    this.psm = { row: i }
  }

  /**
   * Opens PSM `i` among the PSMs of a protein, a peptide, or both (from their detail panes): the PSM table is
   * narrowed to them (`#narrow`, from the search's cutoff) and stays so when the PSM closes; their chips in the
   * filter row widen it again, with the PSM still open.
   */
  openPsmIn(i: number, scope: { protein?: number | null; peptide?: number | null }): void {
    if (!this.#isPsm(i)) return
    const protein = scope.protein ?? null
    const peptide = scope.peptide ?? null
    if (protein !== null && !this.#isProtein(protein)) return
    if (peptide !== null && !this.#isPeptide(peptide)) return
    if (protein !== null) this.#narrow({ protein }, true)
    if (peptide !== null) this.#narrow({ peptide }, protein === null)
    this.openPsm(i)
  }

  /** Opens PSM `i` from the protein panel: among the PSMs of the open protein, and of its peptide `peptide` if given. */
  openPsmFromPanel(i: number, peptide: number | null): void {
    if (this.panel === null) return
    this.openPsmIn(i, { protein: this.panel, peptide })
  }

  /** Closes the PSM view; the PSM stays selected in the PSM table, which keeps its filters. */
  closePsm(): void {
    this.psm = null
  }

  /**
   * The Peptides tab at peptide `k`, its detail open: from a PSM up to its peptide. Filters of the Peptides table
   * that hide the peptide are lifted (as `openPsm` does for a PSM).
   */
  showPeptide(k: number): void {
    if (!this.#isPeptide(k)) return
    const m = this.model
    if (!this.peptides.kept.includes(k)) {
      if (this.peptides.filters.proteinFilter !== undefined) this.widen('protein')
      const f = this.peptides.filters
      this.peptides.filters = {
        ...f,
        text: '',
        maxQ: m.psmFdr,
        proteinFilter: undefined,
        curation: 'any',
        hideContaminants: m.peptide.contaminant[k] ? false : f.hideContaminants,
        showDecoys: m.tables.peptides.decoy[k] ? true : f.showDecoys,
      }
    }
    this.tab = 'peptides'
    this.openPeptide(k)
  }

  /**
   * The Proteins tab at protein `p`, its panel open: from a peptide or a PSM up to its protein. Filters of the
   * Proteins table that hide the protein are lifted.
   */
  showProtein(p: number): void {
    if (!this.#isProtein(p)) return
    const m = this.model
    if (!this.proteins.kept.includes(p)) {
      const f = this.proteins.filters
      this.proteins.filters = {
        ...f,
        text: '',
        maxQ: m.proteinFdr,
        minPeptides: 0,
        minReplicatesInCondition: 0,
        hideContaminants: m.protein.contaminant[p] ? false : f.hideContaminants,
        showDecoys: m.tables.proteins.decoy[p] ? true : f.showDecoys,
      }
    }
    this.tab = 'proteins'
    this.openProtein(p)
  }

  /** The row open in the detail pane of `level`: the protein of the panel, the open peptide, the open PSM; null: none. */
  openRow(level: Level): number | null {
    return level === 'proteins' ? this.panel : level === 'peptides' ? this.peptidePanel : (this.psm?.row ?? null)
  }

  /** The rows before and after the open one in its table as sorted and filtered (null: none). */
  neighbours(level: Level): Neighbours {
    const rows = this[level].rows
    const open = this.openRow(level)
    const position = open === null ? -1 : rows.indexOf(open)
    return {
      prev: position > 0 ? rows[position - 1] : null,
      next: position >= 0 && position + 1 < rows.length ? rows[position + 1] : null,
      position,
      count: rows.length,
    }
  }

  /** Opens the row before (-1) or after (1) the open one of `level`, if there is one. */
  step(level: Level, direction: 1 | -1): void {
    const { prev, next } = this.neighbours(level)
    const target = direction < 0 ? prev : next
    if (target === null) return
    if (level === 'proteins') this.openProtein(target)
    else if (level === 'peptides') this.openPeptide(target)
    else {
      this.psms.selected = target
      this.psm = { row: target }
    }
  }

  /** The PSMs before and after the open one in the PSM table as sorted and filtered (null: none). */
  get psmNeighbours(): Neighbours {
    return this.neighbours('psms')
  }

  /** Opens the PSM before (-1) or after (1) the open one, if there is one. */
  stepPsm(direction: 1 | -1): void {
    this.step('psms', direction)
  }

  #isProtein(p: number): boolean {
    return Number.isInteger(p) && p >= 0 && p < this.model.tables.proteins.count
  }

  #isPeptide(k: number): boolean {
    return Number.isInteger(k) && k >= 0 && k < this.model.tables.peptides.count
  }

  #isPsm(i: number): boolean {
    return Number.isInteger(i) && i >= 0 && i < this.model.tables.psms.count
  }

  /** The level of the active tab, or null on Overview, Samples and QC. */
  get level(): LevelState | null {
    return this.tab === 'proteins' || this.tab === 'peptides' || this.tab === 'psms' ? this[this.tab] : null
  }

  /** The design has technical replicates (a sample measured more than once). */
  get hasTechReplicates(): boolean {
    const seen = new Set<string>()
    for (const row of this.result.design.rows) {
      const key = `${row.sample}\0${row.techReplicate}`
      seen.add(key)
    }
    return seen.size > new Set(this.result.design.rows.map((r) => r.sample)).size
  }
}

/** The view of the open result. Its own effect root owns its deriveds, so that they keep working
 * after the component that first asked for it unmounts (the state survives a visit to New search). */
let cached: { view: ResultsView; destroy: () => void } | null = null

/** The view state of the result open last, if there is one (a setup saves it: `setup/setup-file.svelte.ts`). */
export function currentResultsView(): ResultsView | null {
  return cached?.view ?? null
}

/** Releases the view of the open result (its model and caches), e.g. when another result opens. */
export function releaseResultsView(): void {
  cached?.destroy()
  cached = null
}

/** The view state of `result`: the same object while the same result stays open, a new one for another result. */
export function resultsViewFor(result: OpenResult, store: CurationStore | null = curationStoreFor(result.id)): ResultsView {
  if (cached?.view.result !== result) {
    cached?.destroy()
    let view: ResultsView | undefined
    const destroy = $effect.root(() => {
      view = new ResultsView(result, store)
    })
    // (the server build of Svelte, which the unit tests run on, does not run effect roots)
    cached = { view: view ?? new ResultsView(result, store), destroy }
  }
  return cached.view
}
