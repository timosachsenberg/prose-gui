import { describe, expect, it } from 'vitest'
import type { PsmSpectra } from '../results/read'
import type { ProSEMultiSummary } from '../results/types'
import type { OpenResult } from '../search/session.svelte'
import { sampleDesign, sampleTables } from './fixtures'
import { input } from '../design/fixtures'
import { resolveDesign } from '../design/model'
import { makeTables } from './synthetic'
import { peptidePsms } from './protein'
import { ACCEPTED, NONE, REJECTED } from './curation'
import { releaseResultsView, resultsViewFor, ResultsView, type CurationStore } from './view.svelte'

function result(psmFdr = 0.01, proteinFdr = 0.01, rescored = false, proteinFdrApplied = true): OpenResult {
  const tables = sampleTables()
  return {
    id: 'r1',
    createdAt: '2026-10-17T00:00:00Z',
    design: sampleDesign(),
    psmFdr,
    proteinFdr,
    settings: { parameters: {}, fixedMods: [], database: 'db.fasta', rescore: rescored, peptdeep: false, contaminants: true },
    outcome: {
      tables,
      spectra: { predictions: false, load: () => Promise.reject(new Error('no spectra')) } as PsmSpectra,
      warnings: [],
      proteinsSaved: true,
      seconds: 1,
      summary: {
        files: tables.files.map((path) => ({ path, ms2Spectra: 10, percolator: rescored ? { applied: true } : null })),
        proteinFdrApplied,
        psmFdr,
        proteinFdr,
        decoyString: 'DECOY_',
        openSearch: false,
        modificationAnalysis: null,
      } as unknown as ProSEMultiSummary,
    },
  }
}

const csvLines = (csv: string) => csv.slice(1, -1).split('\n')

/** control 1 measured twice (technical replicates) and EGF 1: PSMs 1, 2 and 3 in the three files. */
function techResult(): OpenResult {
  const r = result(1, 1)
  const design = resolveDesign([
    input('a.mzML', { condition: 'control', replicate: 1, techReplicate: 1 }),
    input('b.mzML', { condition: 'control', replicate: 1, techReplicate: 2 }),
    input('c.mzML', { condition: 'EGF', replicate: 1 }),
  ])
  const tables = makeTables({
    files: ['a.mzML', 'b.mzML', 'c.mzML'],
    proteins: [{ accession: 'P1', sequence: 'AAAAAKCCCCCK' }],
    peptides: [{ sequence: 'AAAAAK', proteins: [0] }],
    psms: [{ peptide: 0, file: 0 }, { peptide: 0, file: 1 }, { peptide: 0, file: 1 }, { peptide: 0, file: 2 }],
  })
  return { ...r, design, outcome: { ...r.outcome, tables } }
}

describe('ResultsView', () => {
  it('is kept while the same result is open, and new for another one', () => {
    const r = result()
    const view = resultsViewFor(r)
    view.tab = 'psms'
    expect(resultsViewFor(r)).toBe(view)
    expect(resultsViewFor(r).tab).toBe('psms')
    const other = resultsViewFor(result())
    expect(other).not.toBe(view)
    expect(other.tab).toBe('proteins')
  })

  it('starts each level at its search cutoff, contaminants hidden, sorted by PSMs or q-value', () => {
    const view = resultsViewFor(result(0.01, 0.003))
    expect(view.proteins.filters).toMatchObject({ maxQ: 0.003, hideContaminants: true, showDecoys: false })
    expect(Array.from(view.proteins.rows)).toEqual([0]) // A1AT is at q 0.004, trypsin a contaminant
    expect(view.proteins.total).toBe(2) // ALBU and the contaminant
    expect(Array.from(view.psms.rows)).toEqual([1, 3, 0, 4, 5, 6]) // by q-value; PSM 7 is trypsin's
    expect(view.peptides.sort).toEqual({ key: 'psms', dir: 'desc' })
  })

  it('sorts a new column by its kind, and flips the same column', () => {
    const view = resultsViewFor(result(1, 1))
    const p = view.proteins
    p.sortBy('psms')
    expect(p.sort).toEqual({ key: 'psms', dir: 'asc' })
    p.sortBy('accession')
    expect(p.sort).toEqual({ key: 'accession', dir: 'asc' })
    p.sortBy('coverage')
    expect(p.sort).toEqual({ key: 'coverage', dir: 'desc' })
    expect(Array.from(p.rows)).toEqual([0, 1]) // 50 % before 25 %
  })

  it('reads one cutoff for the cells, the sort order and the CSV of the peptides', () => {
    const view = resultsViewFor(result(1, 1))
    const p = view.peptides
    p.filters.maxQ = 0.004 // peptide 0: PSMs at 0.004 and 0.001 count, not the one at 0.02
    expect(p.counts.psms[0]).toBe(2)
    expect(Array.from(p.rows)).toEqual([0, 1]) // peptide 2's best PSM is at 0.005, peptide 3 is trypsin's
    const lines = csvLines(p.csv())
    expect(lines[0]).toBe('Peptide,Proteins,PSMs,control 1,EGF 1,Found in,Best PSM q-value,Accepted PSMs,Rejected PSMs')
    expect(lines[1]).toBe('M[Oxidation]KWVTF,sp|P02768|ALBU_HUMAN,2,2,0,1,0.001,0,0')
  })

  it('writes the visible columns only, and the rows of the filtered view', () => {
    const view = resultsViewFor(result(1, 1))
    const p = view.proteins
    p.toggleColumn('samples', false)
    p.toggleColumn('qValue', true)
    p.filters.text = 'albumin'
    expect(csvLines(p.csv())).toEqual([
      'Accession,Description,Coverage (%),Peptides,PSMs,Found in,q-value',
      'sp|P02768|ALBU_HUMAN,"Serum albumin, mature chain",50.0,2,4,2,0.001',
    ])
  })

  it('says why a table is empty', () => {
    const view = resultsViewFor(result(0.01, 0.0001)) // no protein is at q ≤ 0.0001
    expect(view.proteins.rows.length).toBe(0)
    expect(view.proteins.emptyText).toBe("No proteins pass the search's protein FDR (0.01 %, q ≤ 0.0001).")
    view.psms.filters.text = 'nothing like this'
    expect(view.psms.emptyText).toBe('No PSMs match the filters.')
  })

  it('says on every level that no PSM passed the PSM FDR, with the likely cause, and names a protein FDR only if applied', () => {
    const noPsm = "No PSMs pass the search's PSM FDR (0.01 %, q ≤ 0.0001). The database may not match the organism of the sample."
    for (const applied of [true, false]) {
      releaseResultsView()
      const view = resultsViewFor(result(0.0001, 0.0001, false, applied)) // no PSM and no protein is at q ≤ 0.0001
      expect([view.proteins.emptyText, view.peptides.emptyText, view.psms.emptyText]).toEqual([noPsm, noPsm, noPsm])
    }
    // PSMs pass, no protein does, and no protein FDR was applied: the PSM FDR is the cutoff
    releaseResultsView()
    const view = resultsViewFor(result(0.01, 0.0001, false, false))
    expect(view.proteins.emptyText).toBe("No proteins pass the search's PSM FDR (1 %, q ≤ 0.01).")
  })

  it('keeps sorting by the same sample when technical replicates are split or merged', () => {
    const view = resultsViewFor(techResult())
    expect(view.model.samples.map((s) => s.label)).toEqual(['control 1', 'EGF 1'])
    view.proteins.sortBy('sample:1') // EGF 1
    view.peptides.sortBy('sample:0') // control 1
    view.setShowTechReplicates(true)
    expect(view.model.samples.map((s) => s.label)).toEqual(['control 1 · TR1', 'control 1 · TR2', 'EGF 1'])
    expect(view.proteins.sort).toEqual({ key: 'sample:2', dir: 'desc' })
    expect(view.peptides.sort).toEqual({ key: 'sample:0', dir: 'desc' }) // the first measurement of control 1
    view.peptides.sortBy('sample:1') // control 1 · TR2
    view.setShowTechReplicates(false)
    expect(view.proteins.sort).toEqual({ key: 'sample:1', dir: 'desc' })
    expect(view.peptides.sort).toEqual({ key: 'sample:0', dir: 'desc' }) // merged into control 1
    // other sort keys stay
    view.psms.sortBy('charge')
    view.setShowTechReplicates(true)
    expect(view.psms.sort.key).toBe('charge')
  })

  it('releases the view of a result that closed', () => {
    const r = result()
    const view = resultsViewFor(r)
    releaseResultsView()
    expect(resultsViewFor(r)).not.toBe(view)
  })

  it('falls back to the default sort when its sample column goes away', () => {
    const view = resultsViewFor(result(1, 1))
    view.proteins.sortBy('sample:1')
    expect(view.proteins.effectiveSort.key).toBe('sample:1')
    view.proteins.sort = { key: 'sample:5', dir: 'desc' }
    expect(view.proteins.effectiveSort).toEqual({ key: 'psms', dir: 'desc' })
    expect(view.proteins.rows.length).toBe(2)
  })
})

describe('the protein panel', () => {
  it('opens a protein and closes it, and the selection stays', () => {
    const view = resultsViewFor(result(1, 1))
    expect(view.panel).toBeNull()
    expect(view.panelTab).toBe('peptides')
    view.openProtein(1)
    expect(view.panel).toBe(1)
    expect(view.proteins.selected).toBe(1)
    view.openProtein(0) // another protein replaces it
    expect(view.panel).toBe(0)
    expect(view.proteins.selected).toBe(0)
    view.closeProtein()
    expect(view.panel).toBeNull()
    expect(view.proteins.selected).toBe(0) // the row stays selected, the focus goes back to it
  })

  it('opens no panel for what is no protein (a peptide below the protein FDR has none)', () => {
    const view = resultsViewFor(result(1, 1))
    for (const row of [-1, 4, 1.5, NaN, 1e9]) view.openProtein(row)
    expect(view.panel).toBeNull()
    expect(view.proteins.selected).toBeNull()
  })

  it("'Open in PSM table' filters the PSMs by the protein and shows them", () => {
    const view = resultsViewFor(result(1, 1))
    view.filterBy({ protein: 0 })
    expect(view.tab).toBe('psms')
    expect(view.psms.filters.proteinFilter).toBe(0)
    expect(Array.from(view.psms.rows)).toEqual([1, 3, 0, 2]) // ALBU's PSMs, by q-value
    expect(view.psms.rows.length).toBe(view.model.protein.psms[0])
  })

  it("starts from the search's cutoff, without the other filters of that table", () => {
    const view = resultsViewFor(result(1, 1))
    view.psms.filters.text = 'zzz'
    view.psms.filters.maxQ = 0.001
    view.psms.filters.peptideFilter = 0
    view.psms.filters.charges = [7]
    view.filterBy({ protein: 1 })
    expect(view.psms.filters).toMatchObject({ text: '', maxQ: 1, proteinFilter: 1, peptideFilter: undefined, charges: undefined })
    expect(Array.from(view.psms.rows)).toEqual([4, 5, 6])
    expect(view.psms.rows.length).toBe(view.model.protein.psms[1])
  })

  it('shows the contaminants or decoys that its protein is one of', () => {
    const view = resultsViewFor(result(1, 1))
    expect(view.psms.filters).toMatchObject({ hideContaminants: true, showDecoys: false })
    view.filterBy({ protein: 2 }) // the trypsin contaminant
    expect(view.psms.filters.hideContaminants).toBe(false)
    expect(Array.from(view.psms.rows)).toEqual([7])
    view.filterBy({ protein: 3 }) // the decoy
    expect(view.psms.filters.showDecoys).toBe(true)
    expect(Array.from(view.psms.rows)).toEqual([8])
    view.filterBy({ protein: 0 })
    expect(view.psms.filters).toMatchObject({ hideContaminants: false, showDecoys: true }) // what was shown stays shown
  })

  it("'All in the PSM table' of a peptide filters the PSMs by that peptide and shows them", () => {
    const view = resultsViewFor(result(1, 1))
    view.psms.filters.text = 'zzz'
    view.psms.filters.maxQ = 0.001
    view.filterBy({ peptide: 0 }) // M[Oxidation]KWVTF: PSMs 0, 1 and 2
    expect(view.tab).toBe('psms')
    expect(view.psms.filters).toMatchObject({ text: '', maxQ: 1, peptideFilter: 0, proteinFilter: undefined })
    expect(Array.from(view.psms.rows)).toEqual([1, 0, 2]) // by q-value
    expect(view.psms.rows.length).toBe(peptidePsms(view.model, 0).length)
    // a contaminant's or a decoy's peptide is shown, too
    view.filterBy({ peptide: 3 })
    expect(view.psms.filters.hideContaminants).toBe(false)
    expect(Array.from(view.psms.rows)).toEqual([7])
    view.filterBy({ peptide: 4 })
    expect(view.psms.filters.showDecoys).toBe(true)
    expect(Array.from(view.psms.rows)).toEqual([8])
    // the protein filter of an earlier "Open in PSM table" does not stay
    view.filterBy({ protein: 0 })
    view.filterBy({ peptide: 2 })
    expect(view.psms.filters.proteinFilter).toBeUndefined()
    expect(Array.from(view.psms.rows)).toEqual([4, 5, 6])
    // what is no peptide opens nothing
    view.filterBy({ peptide: 99 })
    view.filterBy({ peptide: -1 })
    expect(view.psms.filters.peptideFilter).toBe(2)
  })

  it('opens a PSM in the PSM table: its tab, the PSM selected, the filters that hid it lifted', () => {
    const view = resultsViewFor(result(1, 1))
    view.openPsm(3)
    expect(view.tab).toBe('psms')
    expect(view.psms.selected).toBe(3)
    expect(view.psms.rows.includes(3)).toBe(true)

    // a filter that lets it through stays
    view.psms.filters.text = 'TFISLL'
    view.openPsm(3)
    expect(view.psms.filters.text).toBe('TFISLL')
    // one that hides it goes
    view.openPsm(0)
    expect(view.psms.filters.text).toBe('')
    expect(view.psms.selected).toBe(0)
    view.filterBy({ protein: 1 })
    view.openPsm(0)
    expect(view.psms.filters.proteinFilter).toBeUndefined()
    expect(view.psms.rows.includes(0)).toBe(true)
  })

  it('shows a contaminant PSM or a decoy PSM that the default filters hide', () => {
    const view = resultsViewFor(result(1, 1))
    expect(view.psms.rows.includes(7)).toBe(false)
    view.openPsm(7) // trypsin
    expect(view.psms.filters.hideContaminants).toBe(false)
    expect(view.psms.rows.includes(7)).toBe(true)
    expect(view.psms.rows.includes(8)).toBe(false)
    view.openPsm(8) // the decoy
    expect(view.psms.filters.showDecoys).toBe(true)
    expect(view.psms.rows.includes(8)).toBe(true)
    expect(view.psms.selected).toBe(8)
  })
})

describe('narrowing the tables below a level', () => {
  it('narrows the Peptides and the PSM table to a protein together, and the PSM table to a peptide', () => {
    const view = resultsViewFor(result(1, 1))
    view.peptides.filters.text = 'zzz'
    view.narrowTo({ protein: 0 }, 'peptides')
    expect(view.tab).toBe('peptides')
    expect(view.peptides.filters).toMatchObject({ proteinFilter: 0, text: '' })
    expect(view.psms.filters.proteinFilter).toBe(0)
    expect(Array.from(view.peptides.rows).sort()).toEqual([0, 1]) // ALBU's peptides
    expect(Array.from(view.psms.rows)).toEqual([1, 3, 0, 2])

    // one of its peptides: the PSM table keeps the protein
    view.narrowTo({ peptide: 1 })
    expect(view.tab).toBe('psms')
    expect(view.psms.filters).toMatchObject({ proteinFilter: 0, peptideFilter: 1 })
    expect(Array.from(view.psms.rows)).toEqual([3])
    // a peptide of another protein: the protein goes, from both tables
    view.narrowTo({ peptide: 2 })
    expect(view.psms.filters).toMatchObject({ proteinFilter: undefined, peptideFilter: 2 })
    expect(view.peptides.filters.proteinFilter).toBeUndefined()
    expect(Array.from(view.psms.rows)).toEqual([4, 5, 6])

    // what is no protein or peptide changes nothing
    view.narrowTo({ protein: 99 }, 'peptides')
    view.narrowTo({ peptide: -1 })
    expect(view.tab).toBe('psms')
    expect(view.psms.filters.peptideFilter).toBe(2)
  })

  it('tables that are not narrowed stay as they are when a protein or a peptide opens', () => {
    const view = resultsViewFor(result(1, 1))
    view.openProtein(0)
    view.openPeptide(2)
    expect(view.peptidePanel).toBe(2)
    expect(view.peptides.selected).toBe(2)
    expect(view.peptides.filters.proteinFilter).toBeUndefined()
    expect(view.psms.filters.proteinFilter).toBeUndefined()
    expect(view.psms.filters.peptideFilter).toBeUndefined()
    view.closePeptide()
    expect(view.peptidePanel).toBeNull()
    expect(view.peptides.selected).toBe(2) // the row stays selected
    for (const row of [-1, 5, 1.5, NaN]) view.openPeptide(row)
    expect(view.peptidePanel).toBeNull()
  })

  it('once narrowed, the narrowing follows the protein and the peptide that open, and keeps the other filters', () => {
    const view = resultsViewFor(result(1, 1))
    view.narrowTo({ protein: 0 }, 'peptides')
    view.openPeptide(0)
    view.narrowTo({ peptide: 0 })
    view.psms.filters.maxQ = 0.5
    view.openPsm(1)

    // another protein: both tables follow, the peptide of the old one goes, and what it hides closes
    view.openProtein(1)
    expect(view.peptides.filters.proteinFilter).toBe(1)
    expect(view.psms.filters).toMatchObject({ proteinFilter: 1, peptideFilter: undefined, maxQ: 0.5 })
    expect(Array.from(view.psms.rows)).toEqual([4, 5, 6])
    expect(view.peptidePanel).toBeNull()
    expect(view.psm).toBeNull()
    // the same protein again changes nothing
    view.psms.filters.text = 'q'
    view.openProtein(1)
    expect(view.psms.filters.text).toBe('q')
    view.psms.filters.text = ''

    // a contaminant: its rows show
    expect(view.psms.filters.hideContaminants).toBe(true)
    view.openProtein(2)
    expect(view.psms.filters).toMatchObject({ proteinFilter: 2, hideContaminants: false })
    expect(Array.from(view.psms.rows)).toEqual([7])

    // the PSM table narrowed to a peptide follows the peptide that opens
    view.openProtein(0)
    view.narrowTo({ peptide: 0 })
    view.openPeptide(1)
    expect(view.psms.filters).toMatchObject({ proteinFilter: 0, peptideFilter: 1 })
    expect(Array.from(view.psms.rows)).toEqual([3])
  })

  it('goes up from a PSM to its peptide and from a peptide to its protein, lifting the filters that hide them', () => {
    const view = resultsViewFor(result(1, 1))
    view.peptides.filters.text = 'zzz'
    view.showPeptide(2)
    expect(view.tab).toBe('peptides')
    expect(view.peptidePanel).toBe(2)
    expect(view.peptides.selected).toBe(2)
    expect(view.peptides.filters.text).toBe('')
    // a filter that lets it through stays
    view.peptides.filters.maxQ = 0.5
    view.showPeptide(2)
    expect(view.peptides.filters.maxQ).toBe(0.5)

    expect(view.proteinOf(2)).toBe(1)
    view.proteins.filters.text = 'zzz'
    view.proteins.filters.minPeptides = 5
    view.showProtein(1)
    expect(view.tab).toBe('proteins')
    expect(view.panel).toBe(1)
    expect(view.proteins.filters).toMatchObject({ text: '', minPeptides: 0 })
    expect(view.proteins.rows.includes(1)).toBe(true)
    // a contaminant and a decoy show
    view.showProtein(2)
    expect(view.proteins.filters.hideContaminants).toBe(false)
    view.showPeptide(4)
    expect(view.peptides.filters.showDecoys).toBe(true)
    expect(view.peptidePanel).toBe(4)
    // what is none of them goes nowhere
    view.tab = 'psms'
    view.showProtein(99)
    view.showPeptide(-1)
    expect(view.tab).toBe('psms')
  })

  it('steps through the proteins and the peptides as through the PSMs: the rows of their tables', () => {
    const view = resultsViewFor(result(1, 1))
    const proteins = Array.from(view.proteins.rows)
    expect(view.neighbours('proteins')).toEqual({ prev: null, next: null, position: -1, count: proteins.length })
    view.openProtein(proteins[0])
    expect(view.neighbours('proteins')).toEqual({ prev: null, next: proteins[1], position: 0, count: proteins.length })
    view.step('proteins', 1)
    expect(view.panel).toBe(proteins[1])
    expect(view.proteins.selected).toBe(proteins[1])
    view.step('proteins', 1) // nothing after the last
    expect(view.panel).toBe(proteins[1])

    const peptides = Array.from(view.peptides.rows)
    view.openPeptide(peptides[1])
    expect(view.neighbours('peptides')).toMatchObject({ prev: peptides[0], next: peptides[2], position: 1 })
    view.step('peptides', -1)
    expect(view.peptidePanel).toBe(peptides[0])
    expect(view.peptides.selected).toBe(peptides[0])
    expect(view.openRow('peptides')).toBe(peptides[0])
  })
})

describe('the PSM view', () => {
  it('opens a PSM of the PSM table; Prev and Next follow the rows of the table, as sorted and filtered', () => {
    const view = resultsViewFor(result(1, 1))
    // by q-value; 7 (a contaminant) and 8 (a decoy) are hidden
    expect(Array.from(view.psms.rows)).toEqual([1, 3, 0, 4, 5, 6, 2])
    view.openPsm(3)
    expect(view.psm).toEqual({ row: 3 })
    expect(view.tab).toBe('psms')
    expect(view.psms.selected).toBe(3)
    expect(view.psmNeighbours).toEqual({ prev: 1, next: 0, position: 1, count: 7 })
    view.stepPsm(1)
    expect(view.psm?.row).toBe(0)
    expect(view.psms.selected).toBe(0)
    view.stepPsm(-1)
    view.stepPsm(-1)
    view.stepPsm(-1)
    expect(view.psm?.row).toBe(1)
    expect(view.psmNeighbours.prev).toBeNull()
    view.stepPsm(-1) // nothing before the first
    expect(view.psm?.row).toBe(1)

    // another sort, another order
    view.psms.sortBy('score') // descending: 50 45 42 41 40 40 30
    expect(Array.from(view.psms.rows)).toEqual([1, 3, 4, 5, 0, 6, 2])
    view.openPsm(4)
    expect(view.psmNeighbours).toMatchObject({ prev: 3, next: 5 })
    // a filter that hides the PSM: it stays open, without neighbours
    view.psms.filters.text = 'TFISLL'
    expect(view.psm?.row).toBe(4)
    expect(view.psmNeighbours).toEqual({ prev: null, next: null, position: -1, count: 1 })

    view.closePsm()
    expect(view.psm).toBeNull()
    expect(view.tab).toBe('psms')
    expect(view.psms.selected).toBe(4)
  })

  it('opens a PSM of the protein panel among the PSMs of its peptide; the table stays narrowed, and its chips widen it with the PSM still open', () => {
    const view = resultsViewFor(result(1, 1))
    view.openProtein(0)
    view.panelPeptide = 0
    view.psms.filters.text = 'zzz'
    view.psms.filters.maxQ = 0.005

    view.openPsmFromPanel(0, 0) // PSM 0 of peptide 0 (M[Oxidation]KWVTF of ALBU)
    expect(view.tab).toBe('psms')
    expect(view.psm).toEqual({ row: 0 })
    expect(view.psms.filters).toMatchObject({ proteinFilter: 0, peptideFilter: 0, text: '', maxQ: 1 })
    // (the Peptides table shares the protein)
    expect(view.peptides.filters.proteinFilter).toBe(0)
    expect(Array.from(view.psms.rows)).toEqual([1, 0, 2])
    expect(view.psmNeighbours).toMatchObject({ prev: 1, next: 2, position: 1, count: 3 })

    // the peptide's chip: all PSMs of the protein, the PSM still open, among other neighbours
    view.widen('peptide')
    expect(view.psm).toEqual({ row: 0 })
    expect(Array.from(view.psms.rows)).toEqual([1, 3, 0, 2])
    expect(view.psmNeighbours).toMatchObject({ prev: 3, next: 2, position: 2, count: 4 })
    // the protein's chip: every PSM again, in both tables
    view.widen('protein')
    expect(view.psms.filters.proteinFilter).toBeUndefined()
    expect(view.peptides.filters.proteinFilter).toBeUndefined()
    expect(Array.from(view.psms.rows)).toEqual([1, 3, 0, 4, 5, 6, 2])
    expect(view.psm).toEqual({ row: 0 })
    expect(view.psmNeighbours).toMatchObject({ prev: 3, next: 4 })

    // closing leaves the tab and the table as they are; the protein's panel is as it was, on its tab
    view.closePsm()
    expect(view.psm).toBeNull()
    expect(view.tab).toBe('psms')
    expect(view.psms.selected).toBe(0)
    expect(view.panel).toBe(0)
    expect(view.panelPeptide).toBe(0)

    // from the panel's PSMs tab: all PSMs of the protein
    view.openPsmFromPanel(3, null) // TFISLL of ALBU
    expect(view.psms.filters).toMatchObject({ proteinFilter: 0, peptideFilter: undefined })
    expect(Array.from(view.psms.rows)).toEqual([1, 3, 0, 2])

    // another protein forgets the open peptide
    view.openProtein(1)
    expect(view.panelPeptide).toBeNull()
  })

  it('stays open while another tab shows; closes when a narrowing hides it', () => {
    const view = resultsViewFor(result(1, 1))
    view.openPsm(0)
    // nothing is narrowed: a protein that opens on its tab leaves the PSM open on its own
    view.tab = 'proteins'
    view.openProtein(1)
    expect(view.psm).toEqual({ row: 0 })
    // the PSM table narrowed to a peptide that is not the PSM's: it closes
    view.filterBy({ peptide: 2 })
    expect(view.psm).toBeNull()
    // narrowed to what the PSM is of: it stays
    view.openPsm(4)
    view.filterBy({ protein: 1 })
    expect(view.psm).toEqual({ row: 4 })
    view.closePsm()
    // what is no PSM opens nothing
    view.openPsm(99)
    view.openPsmFromPanel(-1, null)
    view.openPsmIn(0, { protein: 99 })
    view.openPsmIn(0, { peptide: -1 })
    expect(view.psm).toBeNull()
  })

  it('keeps the options of the spectrum from one PSM to the next', () => {
    const view = resultsViewFor(result(1, 1))
    expect(view.psmOptions).toEqual({ ions: ['b', 'y'], charges: null, losses: null, precursor: true, bottom: null })
    view.openPsm(1)
    view.psmOptions.ions = ['a', 'b', 'y']
    view.stepPsm(1)
    expect(view.psmOptions.ions).toEqual(['a', 'b', 'y'])
  })
})

/** A curation store in memory: what it holds, and what was saved. */
function memoryStore(text: string | null = null): CurationStore & { saved: string[] } {
  const saved: string[] = []
  return {
    saved,
    load: () => Promise.resolve(text),
    save: (t: string) => {
      saved.push(t)
      return Promise.resolve()
    },
  }
}

const flush = () => new Promise((resolve) => setTimeout(resolve, 0))

describe('ResultsView curation', () => {
  it('marks PSMs, filters and sorts by the marks, and saves them', async () => {
    const store = memoryStore()
    const view = new ResultsView(result(1, 1), store)
    await view.loaded
    view.curate(1, ACCEPTED)
    view.curate(4, REJECTED)
    view.curate(4, REJECTED) // the same mark: nothing to do
    await flush()
    expect(store.saved.length).toBeGreaterThanOrEqual(1)
    expect(JSON.parse(store.saved.at(-1)!).psms.map((p: { spectrum: string; mark: string }) => [p.spectrum, p.mark])).toEqual([
      ['scan=2', 'accepted'],
      ['scan=5', 'rejected'],
    ])
    view.psms.filters.curation = 'curated'
    expect(Array.from(view.psms.rows)).toEqual([1, 4])
    view.psms.filters.curation = 'any'
    view.psms.sortBy('curation')
    expect(Array.from(view.psms.rows).slice(0, 2)).toEqual([4, 1]) // desc: rejected (2) before accepted (1)
    view.curate(1, NONE)
    expect(Array.from(view.curatedRows())).toEqual([4])
  })

  it('reads the saved marks; what was marked meanwhile wins', async () => {
    const saved = JSON.stringify({
      schema: 1,
      psms: [
        { file: 'ctrl_a.mzML', spectrum: 'scan=1', peptide: 'M[Oxidation]KWVTF', charge: 2, mark: 'accepted' },
        { file: 'ctrl_a.mzML', spectrum: 'scan=2', peptide: 'M[Oxidation]KWVTF', charge: 2, mark: 'accepted' },
        { file: 'gone.mzML', spectrum: 'scan=1', peptide: 'PEPTIDE', charge: 2, mark: 'rejected' },
      ],
    })
    const store = memoryStore(saved)
    const view = new ResultsView(result(1, 1), store)
    view.curate(1, REJECTED) // before the file is read
    await view.loaded
    await flush()
    expect(view.curation[0]).toBe(ACCEPTED)
    expect(view.curation[1]).toBe(REJECTED)
    // the PSM of another file is kept
    const last = JSON.parse(store.saved.at(-1)!)
    expect(last.psms.map((p: { file: string; mark: string }) => [p.file, p.mark])).toEqual([
      ['ctrl_a.mzML', 'accepted'],
      ['ctrl_a.mzML', 'rejected'],
      ['gone.mzML', 'rejected'],
    ])
  })

  it('saves nothing when the file cannot be read, and says so', async () => {
    const store = memoryStore('{')
    const view = new ResultsView(result(1, 1), store)
    await view.loaded
    view.curate(0, ACCEPTED)
    await flush()
    expect(store.saved).toEqual([])
    expect(view.curationError).toContain('could not be read')
  })

  it('goes on to the next PSM when the curation filter hides the one just marked', async () => {
    const view = new ResultsView(result(1, 1))
    view.psms.filters.curation = 'uncurated'
    const rows = Array.from(view.psms.rows)
    view.openPsm(rows[0])
    view.curate(rows[0], ACCEPTED)
    expect(view.psm?.row).toBe(rows[1])
    expect(view.psms.selected).toBe(rows[1])
    // with the filter at "any", the PSM stays open
    view.psms.filters.curation = 'any'
    view.curate(rows[1], REJECTED)
    expect(view.psm?.row).toBe(rows[1])
  })
})

describe('ResultsView: notes and marking several', () => {
  it('keeps a note per PSM, saves it, and reads it back', async () => {
    const store = memoryStore()
    const view = new ResultsView(result(1, 1), store)
    await view.loaded
    view.setNote(2, '  weak  ')
    view.setNote(3, '')
    await flush()
    const saved = store.saved.at(-1)!
    expect(JSON.parse(saved).psms.map((p: { note?: string; mark?: string }) => [p.note, p.mark])).toEqual([['weak', undefined]])
    expect(view.curatedCount()).toBe(1)
    expect(Array.from(view.curatedRows())).toEqual([2])
    view.setNote(2, '')
    expect(view.curatedCount()).toBe(0)

    const again = new ResultsView(result(1, 1), memoryStore(saved))
    await again.loaded
    expect(again.curationNotes.get(2)).toBe('weak')
    expect(again.model.curationNotes.get(2)).toBe('weak')
  })

  it('marks the PSMs shown at once, and takes it back', async () => {
    const store = memoryStore()
    const view = new ResultsView(result(1, 1), store)
    await view.loaded
    view.curate(0, REJECTED)
    const shown = view.psms.rows
    view.curateMany(shown, ACCEPTED)
    expect(Array.from(shown).every((i) => view.curation[i] === ACCEPTED)).toBe(true)
    expect(view.lastBulk?.rows.length).toBe(shown.length)
    view.psms.filters.curation = 'accepted'
    expect(view.psms.rows.length).toBe(shown.length)
    // the peptides with an accepted PSM
    view.peptides.filters.curation = 'accepted'
    expect(view.peptides.rows.length).toBeGreaterThan(0)
    view.undoCurateMany()
    expect(view.curation[0]).toBe(REJECTED)
    expect(Array.from(shown).filter((i) => i !== 0).every((i) => view.curation[i] === NONE)).toBe(true)
    expect(view.lastBulk).toBeNull()
    expect(view.psms.rows.length).toBe(0)
    expect(view.peptides.rows.length).toBe(0)
    await flush()
    expect(JSON.parse(store.saved.at(-1)!).psms.map((p: { mark: string }) => p.mark)).toEqual(['rejected'])
  })
})

