import { readFileSync } from 'node:fs'
import { basename, join } from 'node:path'
import { fileURLToPath } from 'node:url'
import { describe, expect, test } from 'vitest'
import { defaultsOf } from '../settings/parameters'
import { PROSE_PARAMETERS } from '../settings/prose'
import { methodsPlainText, methodsSection, type MethodsInput } from './methods'
import { bytesOfRange, FileNotFound, readSearch, type ByteRange } from './read'
import type { ProSEMultiSummary } from './types'

// The methods section against searches of the pinned ProSE (the reader's fixtures) and against summaries made up to
// reach every case: what ProSE recorded, what it skipped, and the references in the order the text cites them.

const fixtures = fileURLToPath(new URL('./fixtures/', import.meta.url))

async function fixture(name: string) {
  const dir = join(fixtures, name)
  const files = async (n: string, range?: ByteRange) => {
    const path = n.startsWith('/') ? join(dir, 'user', basename(n)) : join(dir, n)
    try {
      return bytesOfRange(new Uint8Array(readFileSync(path)), range)
    } catch (e) {
      if ((e as NodeJS.ErrnoException).code === 'ENOENT') throw new FileNotFound(n)
      throw e
    }
  }
  return readSearch(files)
}

const base = (over: Partial<MethodsInput>): MethodsInput => ({
  summary: {} as ProSEMultiSummary,
  settings: { database: 'db.fasta', contaminants: false, fixedMods: [], parameters: {}, rescore: true, peptdeep: false },
  defaults: defaultsOf(PROSE_PARAMETERS),
  version: '3.7.0',
  appVersion: '0.2.1',
  files: ['a.mzML', 'b.mzML'],
  contaminantProteins: 116,
  ...over,
})

/** A summary of two files, made up: `files` and the rest overridden. */
const summary = (over: Partial<Omit<ProSEMultiSummary, 'files'>> & { files?: unknown[] } = {}) =>
  ({
    files: [
      { ms2Spectra: 100, percolator: { applied: true, skipped: null, warning: null }, peptdeepNce: null, psmFdrApplied: true },
      { ms2Spectra: 100, percolator: { applied: true, skipped: null, warning: null }, peptdeepNce: null, psmFdrApplied: true },
    ],
    search: { enzyme: 'Trypsin', decoyMode: 'generated', targetProteins: 4300, decoyProteins: 4300 },
    openSearch: false,
    psmFdr: 0.01,
    proteinFdr: 0.01,
    proteinFdrApplied: false,
    decoyString: 'DECOY_',
    ...over,
  }) as unknown as ProSEMultiSummary

describe('the example search of the pinned ProSE', async () => {
  const read = await fixture('example')
  const methods = methodsSection(
    base({
      summary: read.summary,
      provenance: read.provenance,
      version: read.toolVersion ?? '',
      settings: { database: 'Ecoli_K12_proteome.fasta', contaminants: true, fixedMods: ['Carbamidomethyl (C)'], parameters: {}, rescore: true, peptdeep: true },
      files: ['Ecoli_1.mzML', 'Ecoli_2.mzML'],
      instruments: { 'Ecoli_1.mzML': 'NT=LTQ Orbitrap XL;AC=MS:1000556', 'Ecoli_2.mzML': 'NT=LTQ Orbitrap XL;AC=MS:1000556' },
    }),
  )
  const [search, filtering] = methods.paragraphs

  test('says what was searched, with what and against what', () => {
    expect(search).toContain(`Two LC-MS/MS runs (mzML files; LTQ Orbitrap XL, as recorded in the files) were searched with ProSE (OpenMS ${read.toolVersion}) [1]`)
    expect(search).toContain('against the protein database Ecoli_K12_proteome.fasta.')
    expect(search).toContain('The 116 common contaminant sequences of cRAP (thegpm.org/crap) were appended, giving 4,252 target sequences.')
  })

  test('gives the settings ProSE recorded', () => {
    expect(search).toContain('Trypsin/P was set as the enzyme with full specificity and up to 2 missed cleavages; peptides of 7 to 40 residues were considered.')
    expect(search).toContain('Protein N-terminal peptides were also searched without their initial methionine.')
    // (the tool's defaults since OpenMS #10804 and #10805: isotope errors from −2, the calibration pass, the m/z correction)
    expect(search).toContain('The precursor mass tolerance was ±10 ppm, with precursor isotope errors from −2 to +1 (¹³C spacings) considered, and the fragment mass tolerance 0.5 Da.')
    expect(search).toContain(
      'Before the main search, a calibration pass on a subset of the spectra measured the mass errors: within the tolerances set, the fragment tolerance was narrowed to them where enough confident matches were found and the m/z values were corrected for the median error measured.',
    )
    expect(search).toContain('Precursor charges 2 to 5 were searched, and fragment ions up to charge 2 were scored.')
    expect(search).toContain(
      'Carbamidomethyl (C) was set as a fixed modification, and Oxidation (M) and Acetyl (Protein N-term) were searched as variable modifications (at most 2 per peptide).',
    )
    expect(search).toContain('so that each target peptide had a decoy of the same length and mass [2].')
  })

  test('says how the PSMs were rescored and filtered, and what was not FDR-controlled', () => {
    expect(filtering).toContain(
      "PSMs were rescored with Percolator [3] as implemented in OpenMS, on ProSE's PSM features and the fragment-intensity and retention-time predictions of AlphaPeptDeep [4] (MS2 model for QE instruments, NCE 37, chosen from the spectra).",
    )
    // (the fixture's runs are too small for Percolator's learned model)
    expect(filtering).toContain("For both runs, Percolator's check of the learned model on held-out data rejected it")
    expect(filtering).toContain('PSMs were filtered at 1% PSM-level FDR (Percolator q-values, estimated for each run separately).')
    expect(filtering).toContain('No peptide-level FDR was applied')
    expect(filtering).toContain('No protein-level FDR was applied; the reported proteins and protein groups are not FDR-controlled.')
    expect(filtering).toContain('The results were inspected with the ProSE desktop app 0.2.1')
  })

  test('numbers the references in the order the text cites them, and leaks no local path', () => {
    expect(methods.references.map((r) => r.split('.')[0])).toEqual(['Pfeuffer J, Bielow C, et al', 'Elias JE, Gygi SP', 'Käll L, Canterbury JD, Weston J, Noble WS, MacCoss MJ', 'Zeng W-F, Zhou X-X, Willems S, et al'])
    const text = methodsPlainText(methods)
    expect(text).toContain('\n\nReferences\n\n[1] Pfeuffer J')
    expect(text).toContain('[4] Zeng W-F')
    expect(text).not.toMatch(/\/tmp\/|\/home\//)
  })
})

test('an open search names its window and the unlocalized mass shifts', async () => {
  const read = await fixture('open')
  const [search, filtering] = methodsSection(base({ summary: read.summary, provenance: read.provenance, files: ['a.mzML'] })).paragraphs
  expect(search).toContain('One LC-MS/MS run (mzML file) was searched')
  expect(search).toContain('An open search was performed with a precursor mass window of ±500 Da; the mass shift of each match was reported unlocalized.')
  expect(search).toContain('Carbamidomethyl (C) was set as a fixed modification; no variable modifications were searched.')
  expect(filtering).not.toContain('Percolator')
  expect(filtering).toContain('q-values from target-decoy competition [2]')
})

describe('what ProSE applied, and only that', () => {
  test('a protein FDR applied: the picked-protein approach on the pooled PSMs', () => {
    const [, filtering] = methodsSection(base({ summary: summary({ proteinFdrApplied: true }) })).paragraphs
    expect(filtering).toContain('Proteins were inferred from the pooled PSMs of all runs with q ≤ 0.1, indistinguishable proteins grouped, and protein groups filtered at 1% protein-level FDR with the picked-protein approach')
    expect(filtering).not.toContain('estimated for each run separately')
    expect(filtering).not.toContain('No protein-level FDR')
  })

  test('no decoys, or a PSM FDR every file skipped: nothing is called FDR-controlled', () => {
    const none = methodsSection(base({ summary: summary({ search: { enzyme: 'Trypsin', decoyMode: 'none (target-only)', targetProteins: 10, decoyProteins: 0 } as never }) })).paragraphs.join(' ')
    expect(none).toContain('The database was searched without decoy sequences.')
    expect(none).toContain('No false discovery rate was estimated')
    expect(none).not.toContain('PSM-level FDR (')
    const skipped = summary()
    for (const f of skipped.files) Object.assign(f, { psmFdrApplied: false, percolator: { applied: false, skipped: 'no decoys', warning: null } })
    const text = methodsSection(base({ summary: skipped })).paragraphs.join(' ')
    expect(text).toContain('not applied (no decoy PSMs), so the PSMs are not FDR-controlled')
    expect(text).toContain('Percolator was not applied to both runs')
    expect(text).not.toContain('rescored with Percolator')
  })

  test('Percolator for some runs only; the decoys of the database', () => {
    const s = summary({ search: { enzyme: 'Trypsin', decoyMode: 'external', targetProteins: 10, decoyProteins: 10 } as never })
    Object.assign(s.files[1], { percolator: { applied: false, skipped: 'too few PSMs', warning: null } })
    const text = methodsSection(base({ summary: s })).paragraphs.join(' ')
    expect(text).toContain('for 1 of 2 runs.')
    // (a count that starts the sentence: spelled out up to ten)
    const twelve = summary({ files: Array.from({ length: 12 }, () => ({ ms2Spectra: 1, percolator: null, peptdeepNce: null, psmFdrApplied: true })) })
    expect(methodsSection(base({ summary: twelve, files: Array.from({ length: 12 }, (_, i) => `r${i}.raw`) })).paragraphs[0]).toMatch(/^A total of 12 LC-MS\/MS runs \(Thermo \.raw files\)/)
    expect(text).toContain('The decoy sequences contained in the database (marked DECOY_) were used [2].')
  })

  test('a red flag searched anyway is named only when the search confirmed it', () => {
    const accepted = [{ id: 'profile-ms2', files: ['/r/a.mzML'], message: 'Profile.' }]
    const confirmed = summary()
    Object.assign(confirmed.files[0], { profileSpectra: 12 })
    expect(methodsSection(base({ summary: confirmed, accepted })).paragraphs[1]).toContain('recorded in profile mode')
    expect(methodsSection(base({ summary: summary(), accepted })).paragraphs[1]).not.toContain('profile mode')
  })
})

describe('what the author has to add', () => {
  test('a UniProt proteome: its organism, and the release to fill in', () => {
    const m = methodsSection(base({ summary: summary(), settings: { ...base({}).settings, database: '/home/me/UP000005640_reviewed.fasta' } }))
    expect(m.paragraphs[0]).toContain('against the UniProt reference proteome UP000005640 (Homo sapiens; Swiss-Prot entries only; release [YYYY_MM]).')
    expect(m.paragraphs.join(' ')).not.toContain('/home/me')
    expect(m.notes[0]).toContain('Fill in the UniProt release')
  })

  test('another FASTA: its source and version; an unknown ProSE version: a placeholder', () => {
    const m = methodsSection(base({ summary: summary(), version: '' }))
    expect(m.notes.some((n) => n.startsWith('Name the source and version of db.fasta'))).toBe(true)
    expect(m.paragraphs[0]).toContain('ProSE (OpenMS [version]) [1]')
    expect(m.notes.some((n) => n.includes('version of ProSE that searched is not recorded'))).toBe(true)
  })

  test('the settings the search was started with fill what the provenance lacks: semi-specific, no mods', () => {
    const m = methodsSection(
      base({
        summary: summary(),
        settings: { ...base({}).settings, parameters: { 'peptide:enzyme_specificity': 'semi', 'modifications:fixed': [], 'modifications:variable': [], 'precursor:mass_tolerance_lower': 5, 'precursor:mass_tolerance_upper': 20 } },
      }),
    )
    expect(m.paragraphs[0]).toContain('Trypsin was set as the enzyme with semi-specific cleavage (one enzymatic terminus)')
    expect(m.paragraphs[0]).toContain('−5 to +20 ppm')
    expect(m.paragraphs[0]).toContain('No modifications were searched.')
    // (semi-specific: the decoys are reversed peptides, not keep_termini's)
    expect(m.paragraphs[0]).toContain('by reversing each enzymatic peptide except its C-terminal residue')
  })
})
