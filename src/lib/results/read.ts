// The results reader: ProSE's QPX outputs in a search folder (docs/parity.md) as the web app's result types,
// so that the web app's screens show them unchanged.
//
// The folder is the shim's (src-tauri/src/run/folder.rs, args/inputs.rs): inputs.json (the inputs in order, with the
// links `<n>__<name>` ProSE saw and their stems), and in qpx/ the QPX views of ProSE's -qpx_extended:
// quantms.psm.parquet (the PSM rows of all inputs, in input order, with the optional columns but without the
// spectra), quantms.run.parquet (one row per input: its statistics and its rows in quantms.psm.parquet),
// quantms.provenance.parquet (the steps of the search with their parameters), quantms.pg.parquet (the protein
// groups: of several inputs the pooled inference), and of each input in qpx/runs/ <stem>.psm.parquet (the same rows
// with PeptDeep's prediction) and <stem>.mz.parquet (the QPX mz view of the spectra of its PSMs, filtered peaks), read
// by row group when a PSM is opened (a search of app 0.1.x has them in qpx/, next to quantms.*); mods/ (open search: the
// tables the provenance names), and database.fasta (with the contaminants). Without contaminants ProSE
// searched the user's FASTA: the reader asks for the path the provenance records (`database_file`), an
// absolute path, which the `files` callback has to serve too. The protein sequences and descriptions come
// from proteins.fasta (the identified proteins, if the folder has it), else the database, else they are left
// out with a warning (`SearchResult.warnings`).
//
// The `files` contract: resolve with the contents (all of the file, or the byte range asked for), or reject
// with `FileNotFound` (or any error or object with `notFound: true`, as a Tauri command can send it) when the
// file does not exist. Only that counts as a missing file; any other rejection fails the read, with the
// file's name in the message. A file that is there but cannot be read fails the read too, naming the file.
//
// The parts: search-folder.ts reads the folder's files and the PSM rows of a psm file, psm-spectra.ts the
// spectra of a PSM when it is opened; this module makes the tables. What they export for the app is
// exported here.

import type { FileMetaData } from 'hyparquet'
import { errorText } from '../format/error'
import {
  C13C12_MASSDIFF_U,
  countEnzymatic,
  generatedDecoy,
  isEnz,
  modificationMass,
  openmsString,
  pinEnzymeName,
  proformaString,
  type DecoyGeneration,
  type Modification,
} from './derive'
import {
  columnRuns,
  metadataOfTail,
  openParquet,
  partsOfParquet,
  readLeaf,
  readStructList,
  startZstd,
  type ParquetFile,
} from './parquet'
import { CHIMERIC_RANK } from './coisolated'
import { parameterFlag, parameterNumber, parseModificationAnalysis, readMeta, toSummary, type SearchMeta } from './summary'
import type { ProSESummary, ProSEMultiSummary, ResultTables } from './types'
import { psmSpectra, type PsmSpectra } from './psm-spectra'
import {
  EXTENDED_COLUMNS,
  FileNotFound,
  first,
  GROUPS,
  inFile,
  isNotFound,
  MERGED_PSMS,
  PROVENANCE,
  readRows,
  ROW_COLUMNS,
  RUNS,
  type ByteRange,
  type Modifications,
  type PsmRows,
  type SearchFiles,
} from './search-folder'

export { ZSTD } from './parquet'
export { bytesOfRange, FileNotFound, isNotFound, MERGED_PSMS, PROVENANCE, RUNS, type ByteRange, type SearchFiles } from './search-folder'
export type { LoadedSpectra, PeakFilter, PsmSpectra } from './psm-spectra'

/**
 * A search folder of the app's earlier layouts (summary.yaml and OpenMS' own Parquet files next to the QPX
 * files; or QPX files without the run and provenance views), which this version does not read.
 */
export class OldLayoutError extends Error {
  readonly oldLayout = true
  constructor() {
    super(
      'This search was saved by an earlier version of ProSE Desktop, whose results this version cannot read. ' +
        'Open it with that version, or run the search again.',
    )
    this.name = 'OldLayoutError'
  }
}

export interface SearchResult {
  summary: ProSEMultiSummary
  tables: ResultTables
  /** the peaks and predictions of the PSMs (read when a PSM is opened) */
  spectra: PsmSpectra
  /** what could not be read and was left out, for the user (e.g. the protein sequences) */
  warnings: string[]
  /**
   * The protein sequences came from the folder's proteins.fasta, or there are no proteins: History need not
   * write proteins.fasta for this search (it does when this is false: history.ts, `openSearch`).
   */
  proteinsSaved: boolean
  /** ProSE's parameters of the search as its provenance view records them (step peptide_search), by key. */
  provenance?: Record<string, string>
  /** The version of ProSE that searched, as its provenance view records it (step peptide_search); absent: not recorded. */
  toolVersion?: string
}

/** A record of inputs.json (args/inputs.rs `InputRecord`). */
interface InputRecord {
  id: string
  index: number
  link: string
  stem: string
  name: string
  path: string
}

const utf8 = new TextDecoder()

/** ProSE's ln(hyperscore): the name of its feature column (as the web app names it) */
const HYPERSCORE = 'ln(hyperscore)'
const PEPTDEEP_FEATURES = ['ms2_cosine', 'ms2_spectral_angle', 'ms2_pearson', 'ms2_frac_pred_found', 'rt_abs_error']

/** The larger of two numbers, ignoring NaN (C's fmax) */
const fmax = (a: number, b: number) => (Number.isNaN(a) ? b : Number.isNaN(b) ? a : Math.max(a, b))

/** A psm file without the columns of -qpx_extended. */
class NotExtended extends Error {}

/**
 * The fewest rows read at a time: the row groups of quantms.psm.parquet are read one after the other (ProSE writes
 * about a million rows per group), small ones together until they have this many rows.
 */
const BATCH_ROWS = 1 << 17

/** The metadata of a Parquet file from its last bytes (its footer); null if the file is not there. */
async function metadataOf(files: SearchFiles, name: string): Promise<FileMetaData | null> {
  const tail = async (range: ByteRange) => {
    try {
      return await files(name, range)
    } catch (e) {
      if (isNotFound(e)) return null
      throw new Error(`${name}: ${errorText(e)}`, { cause: e })
    }
  }
  const first = await tail({ tail: 1 << 16 })
  if (!first) return null
  return inFile(name, async () => {
    let metadata = metadataOfTail(first)
    if (typeof metadata === 'number') {
      const more = await tail({ tail: metadata })
      if (!more) throw new FileNotFound(name)
      metadata = metadataOfTail(more)
    }
    if (typeof metadata === 'number') throw new Error('its footer cannot be read')
    return metadata
  })
}

/**
 * The row groups of a file in batches of at least `batchRows` rows (the last may have fewer), in order: the
 * groups of each batch and its first row.
 */
function batchesOf(metadata: FileMetaData, batchRows: number): { groups: number[]; from: number; rows: number }[] {
  const batches: { groups: number[]; from: number; rows: number }[] = []
  let from = 0
  let current: { groups: number[]; from: number; rows: number } | null = null
  metadata.row_groups.forEach((group, g) => {
    if (!current) batches.push((current = { groups: [], from, rows: 0 }))
    current.groups.push(g)
    current.rows += Number(group.num_rows)
    from += Number(group.num_rows)
    if (current.rows >= batchRows) current = null
  })
  return batches
}

/** The columns `columns` of the row groups `groups` of the file `name`: only their bytes are read. */
async function readColumns(files: SearchFiles, name: string, metadata: FileMetaData, groups: number[], columns: string[]): Promise<ParquetFile> {
  const parts = await Promise.all(
    columnRuns(metadata, groups, columns).map(async ({ start, end }) => {
      const bytes = await files(name, { offset: start, length: end - start })
      if (bytes.byteLength !== end - start) throw new Error(`${bytes.byteLength} bytes at ${start}, expected ${end - start}`)
      return { bytes, base: start }
    }),
  )
  return partsOfParquet(metadata, parts)
}

//------------------------------------------------------------------------------
// FASTA

interface FastaEntry {
  description: string
  sequence: string
}

/** The entries of a FASTA file by identifier (the header up to the first white space, as OpenMS' FASTAFile). */
function readFasta(text: string): Map<string, FastaEntry> {
  const entries = new Map<string, FastaEntry>()
  let id: string | null = null
  let description = ''
  let parts: string[] = []
  const flush = () => {
    if (id !== null && !entries.has(id)) entries.set(id, { description, sequence: parts.join('') })
  }
  for (const raw of text.split('\n')) {
    const line = raw.trim()
    if (line.startsWith('>')) {
      flush()
      const header = line.slice(1).trim()
      const space = header.search(/\s/)
      id = space < 0 ? header : header.slice(0, space)
      description = space < 0 ? '' : header.slice(space).trim()
      parts = []
    } else if (id !== null && line !== '') {
      parts.push(line.replace(/\s+/g, ''))
    }
  }
  flush()
  return entries
}

/** The sequence as ProSE searches it: without the stop codons ('*') at its end. */
const searched = (sequence: string) => sequence.replace(/\*+$/, '')

/** The protein q-value of a pg row's additional_scores: the entry named q-value (any spelling). */
const isQValueName = (name: string) => name.toLowerCase().replace(/[^a-z]/g, '') === 'qvalue'

//------------------------------------------------------------------------------
// The reader

/**
 * Reads the search folder that `files` serves. The PSM rows (quantms.psm.parquet) are read a batch of row groups
 * at a time, of at least `options.batchRows` rows (BATCH_ROWS), and only the columns the tables are made of: the
 * file is never in memory as a whole, and what is decoded of a batch is let go before the next.
 */
export async function readSearch(files: SearchFiles, options: { batchRows?: number } = {}): Promise<SearchResult> {
  const zstd = startZstd()
  const warnings: string[] = []
  /** a file that may be missing: null if it does not exist */
  const optional = (name: string, range?: ByteRange) =>
    files(name, range).catch((e: unknown) => {
      if (isNotFound(e)) return null
      throw new Error(`${name}: ${errorText(e)}`, { cause: e })
    })
  const required = async (name: string) => {
    const bytes = await optional(name)
    if (!bytes) throw new FileNotFound(name)
    return bytes
  }

  const inputs = await inFile('inputs.json', async () =>
    (JSON.parse(utf8.decode(await required('inputs.json'))) as InputRecord[]).sort((a, b) => a.index - b.index),
  )
  await zstd

  // quantms.psm.parquet is not read as a whole: its footer here, its rows below, a batch of row groups at a time
  const [runBytes, provenanceBytes, psmMetadata, groupBytes] = await Promise.all([
    optional(RUNS),
    optional(PROVENANCE),
    metadataOf(files, MERGED_PSMS),
    optional(GROUPS),
  ])
  if (!runBytes || !provenanceBytes) {
    // the earlier layouts of the app's search folders: summary.yaml, or QPX files without these views
    if (psmMetadata || (await optional('summary.yaml'))) throw new OldLayoutError()
    throw new Error(`The search folder has no results of ProSE (${RUNS}, ${PROVENANCE}).`)
  }
  const meta: SearchMeta = await inFile(`${RUNS}, ${PROVENANCE}`, () => readMeta(runBytes, provenanceBytes))
  if (meta.runs.length !== inputs.length) {
    throw new Error(`${RUNS}: ${meta.runs.length} runs, but the search has ${inputs.length} input(s).`)
  }
  if (psmMetadata) {
    const columns = new Set(psmMetadata.schema.map((e) => e.name))
    const missing = EXTENDED_COLUMNS.filter((c) => !columns.has(c))
    if (missing.length > 0) {
      if (await optional('summary.yaml')) throw new OldLayoutError()
      throw new NotExtended(`${MERGED_PSMS}: the QPX psm view has not the columns of ProSE's -qpx_extended`)
    }
  }
  // the rows of each input in quantms.psm.parquet: input k has rows from[k] .. from[k + 1]
  const from = [0]
  meta.runs.forEach((r) => from.push(from[from.length - 1] + r.psm_rows))
  const count = psmMetadata ? Number(psmMetadata.num_rows) : 0
  if (from[inputs.length] !== count) {
    throw new Error(`${MERGED_PSMS}: ${count} rows, but ${RUNS} gives its inputs ${from[inputs.length]}.`)
  }
  // an input without rows whose search reports PSMs: its psm file could not be written
  inputs.forEach((input, k) => {
    const r = meta.runs[k]
    if (r.psm_rows === 0 && r.search_status === 'EXECUTION_OK' && r.identified_spectra_count > 0) {
      warnings.push(
        `${input.name}: ProSE reports ${r.identified_spectra_count} target PSMs for it, but its results are not in the search folder, so its PSMs are not shown.`,
      )
    }
  })
  const S = meta.search
  const decoyString = S.get('decoy_string') ?? 'DECOY_'
  const decoyIsPrefix = (S.get('decoy_position') ?? 'prefix') === 'prefix'
  const hasDecoyMarker = (accession: string) =>
    decoyString !== '' && (decoyIsPrefix ? accession.startsWith(decoyString) : accession.endsWith(decoyString))
  const proteinFdrApplied = parameterFlag(meta.fdr, 'protein_fdr_applied')

  //------------------------------------------------------------------------------
  // Proteins and their groups: the protein groups of the QPX pg view (several inputs: the pooled inference;
  // without protein inference, one group per protein), with the FASTA

  const accession: string[] = []
  const ownDescription: string[] = []
  const proteinQ: number[] = []
  const proteinDecoy: number[] = []
  const group: number[] = []
  const proteinRow = new Map<string, number>()
  if (groupBytes) {
    const bytes = groupBytes
    await inFile(GROUPS, () => {
      const pg = openParquet(bytes)
      const members = readLeaf(pg, ['pg_accessions', 'list', 'element'])
      const names = readLeaf(pg, ['pg_names', 'list', 'element'])
      const isDecoy = readLeaf(pg, ['is_decoy']).values as unknown[]
      const groupQ = readLeaf(pg, ['global_qvalue']).values as Float64Array
      // the q-value of the group's first protein (once the protein FDR was applied): its additional_scores
      // entry 'q-value'
      const scores = readStructList(pg, 'additional_scores', ['score_name', 'score_value'], { score_name: 'codes' })
      const qName = (scores.dictionaries.score_name ?? []).map(isQValueName)
      const groupOf = new Map<string, number>()
      for (let r = 0; r < pg.rows; r++) {
        const begin = members.offsets![r]
        const end = members.offsets![r + 1]
        const list = Array.from({ length: end - begin }, (_, k) => String((members.values as unknown[])[begin + k]))
        // a group of several runs has a row per run
        const key = list.join('\u0000')
        let g = groupOf.get(key)
        if (g === undefined) {
          g = groupOf.size
          groupOf.set(key, g)
        }
        let q = groupQ[r]
        for (let k = scores.offsets[r]; k < scores.offsets[r + 1]; k++) {
          const c = (scores.score_name as Int32Array)[k]
          if (c >= 0 && qName[c]) {
            q = (scores.score_value as Float64Array)[k]
            break
          }
        }
        list.forEach((a, k) => {
          if (proteinRow.has(a)) return
          proteinRow.set(a, accession.length)
          accession.push(a)
          const own = names.offsets ? (names.values as unknown[])[names.offsets[r] + k] : null
          ownDescription.push(typeof own === 'string' ? own : '')
          proteinQ.push(proteinFdrApplied ? q : NaN)
          proteinDecoy.push(hasDecoyMarker(a) || (isDecoy[r] === true && list.length === 1) ? 1 : 0)
          group.push(g)
        })
      }
    })
  }
  const proteinCount = accession.length
  let proteinsSaved = proteinCount === 0
  const description = ownDescription.slice()
  const proteinSequence = new Array<string>(proteinCount).fill('')
  const proteinScore = new Float64Array(proteinCount).fill(NaN)
  if (proteinCount > 0) {
    // the sequences and descriptions: proteins.fasta (the identified proteins), else the database (the
    // folder's database.fasta with contaminants, else the user's FASTA), else none, with a warning
    let fasta = new Map<string, FastaEntry>()
    let identified = false
    const tried: string[] = []
    const sources: [string, boolean][] = [
      ['proteins.fasta', true],
      ['database.fasta', false],
      [S.get('database_file') ?? '', false],
    ]
    let found = false
    for (const [name, own] of sources) {
      if (name === '') continue
      try {
        const bytes = await files(name)
        fasta = readFasta(utf8.decode(bytes))
        identified = own
        proteinsSaved = own
        found = true
        break
      } catch (e) {
        tried.push(isNotFound(e) ? `${name} (not found)` : `${name} (${errorText(e)})`)
      }
    }
    if (!found) {
      warnings.push(`The protein sequences are not available, so there is no sequence coverage: none of ${tried.join(', ')} could be read.`)
    }
    const decoys: DecoyGeneration = {
      enzyme: S.get('enzyme') ?? '',
      keepInitialMet: parameterFlag(S, 'clip_nterm_methionine'),
      unspecific: S.get('enzyme_specificity') === 'none',
    }
    const generated = S.get('decoy_mode') === 'generated'
    for (let p = 0; p < proteinCount; p++) {
      const a = accession[p]
      const decoy = proteinDecoy[p] === 1
      // proteins.fasta has the proteins as searched, decoys included
      const listed = identified ? fasta.get(a) : undefined
      // a decoy ProSE generated is the decoy marker + the accession of its target (whose description it
      // gets, as in the web app); generated decoys also replace decoys of the FASTA
      const ofTarget = !listed && decoy && generated && decoyIsPrefix && hasDecoyMarker(a)
      const entry = listed ?? (ofTarget ? fasta.get(a.slice(decoyString.length)) : fasta.get(a))
      if (entry) {
        proteinSequence[p] = ofTarget ? generatedDecoy(searched(entry.sequence), decoys) : searched(entry.sequence)
        if (description[p] === '') description[p] = entry.description
      }
    }
  }

  //------------------------------------------------------------------------------
  // PSMs and their peptides

  // the charges of the PSMs (for the charge<N> features): the column alone, of every row group
  let minCharge = parameterNumber(S, 'min_charge', Infinity)
  let maxCharge = parameterNumber(S, 'max_charge', -Infinity)
  const batches = psmMetadata ? batchesOf(psmMetadata, options.batchRows ?? BATCH_ROWS) : []
  if (psmMetadata) {
    const metadata = psmMetadata
    await inFile(MERGED_PSMS, async () => {
      for (const batch of batches) {
        const file = await readColumns(files, MERGED_PSMS, metadata, batch.groups, ['charge'])
        for (const c of readLeaf(file, ['charge'], 'values', batch.groups).values as Float64Array) {
          minCharge = Math.min(minCharge, c)
          maxCharge = Math.max(maxCharge, c)
        }
      }
    })
  }

  // the features: ln(hyperscore), the standard .pin features (PercolatorInfile::getStandardFeatureSet
  // without the bookkeeping columns), the run's extra_features, PeptDeep's; those some PSM has
  const featureNames: string[] = [HYPERSCORE]
  const pin = new Set<string>()
  const addFeature = (name: string, isPin = false) => {
    if (name === '' || name === 'score' || name === HYPERSCORE || name === 'ln_hyperscore' || featureNames.includes(name)) return
    featureNames.push(name)
    if (isPin) pin.add(name)
  }
  const chargeFeatures: string[] = []
  for (let c = minCharge; Number.isFinite(c) && c <= maxCharge; c++) chargeFeatures.push(`charge${c}`)
  for (const name of ['CalcMass', 'mass', 'peplen', ...chargeFeatures, 'enzN', 'enzC', 'enzInt', 'dm', 'absdm']) addFeature(name, true)
  for (const name of (S.get('extra_features') ?? '').split(',')) addFeature(name.trim())
  for (const name of PEPTDEEP_FEATURES) addFeature(name)
  const nFeatures = featureNames.length
  const featureIndex = new Map(featureNames.map((n, j) => [n, j]))

  // the scores the columns need, by their QPX names: a slot of `values` below
  const SCORES = [
    'ln_hyperscore',
    'percolator_score',
    'global_qvalue',
    'percolator_qvalue',
    'percolator_pep',
    'precursor_mz_error_ppm',
    'isotope_error',
    'delta_mass',
  ] as const
  type ScoreName = (typeof SCORES)[number]
  const scoreSlot = new Map<string, number>(SCORES.map((n, k) => [n, k]))
  const values = new Float64Array(SCORES.length)
  const slot = (n: ScoreName) => scoreSlot.get(n)!
  const featureOfScore = new Map<string, number>()
  for (const [n, j] of featureIndex) if (!pin.has(n) && j > 0) featureOfScore.set(n, j)
  /** what a score name is for: [its slot or −1, its feature column or −1]; null: nothing */
  const uses = new Map<string, [number, number] | null>()
  const useOf = (name: string) => {
    let use = uses.get(name)
    if (use === undefined) {
      const [s, j] = [scoreSlot.get(name) ?? -1, featureOfScore.get(name) ?? -1]
      use = s < 0 && j < 0 ? null : [s, j]
      uses.set(name, use)
    }
    return use
  }
  // the columns of the .pin features
  const pinColumn = (name: string) => featureIndex.get(name)!
  const [calcMassJ, massJ, peplenJ, enzNJ, enzCJ, enzIntJ, dmJ, absdmJ] = ['CalcMass', 'mass', 'peplen', 'enzN', 'enzC', 'enzInt', 'dm', 'absdm'].map(pinColumn)
  const chargeColumns = chargeFeatures.map(pinColumn)
  const [hyperscoreS, percolatorScoreS, qS, percolatorQS, percolatorPepS, precursorErrorS, isotopeErrorS, deltaMassS] = SCORES.map(slot)
  const enzyme = pinEnzymeName(S.get('enzyme') ?? '')
  // QPX has the isotope error observed minus theoretical (column and additional_scores); Percolator saw
  // ProSE's meta value, whose sign 'report:isotope_error_convention' sets: its feature and dm have that sign
  const isotopeSign = S.get('isotope_error_convention') === 'theoretical_minus_observed' ? -1 : 1
  const isotopeFeatureJ = featureIndex.get('isotope_error') ?? -1

  const psmFile = new Uint16Array(count)
  const nativeId = new Array<string>(count)
  const rt = new Float64Array(count)
  const mz = new Float64Array(count)
  const charge = new Int8Array(count)
  const peptide = new Uint32Array(count)
  const score = new Float64Array(count)
  const qValue = new Float64Array(count)
  const pep = new Float64Array(count)
  const precursorErrorPpm = new Float64Array(count)
  const isotopeError = new Int8Array(count)
  const deltaMass = new Float64Array(count)
  const decoy = new Uint8Array(count)
  const psmId = new Float64Array(count)
  // the features of the PSMs are not kept (some 35 doubles per PSM, most of what the rows would take): the tables
  // have their names, those some PSM has (`featurePresent`), and ProSE's rank of the co-isolated precursors, which
  // the PSM table shows; the others are read with the spectrum of a PSM, when it is opened (PsmSpectra.load)
  const featurePresent = new Uint8Array(nFeatures)
  featurePresent[0] = 1
  const rankJ = featureIndex.get(CHIMERIC_RANK) ?? -1
  const rank = rankJ >= 0 ? new Float32Array(count).fill(NaN) : null
  /** the features of the row `evaluate` was last given, by `featureNames` (NaN: absent) */
  const scratch = new Float64Array(nFeatures)
  const usesOf = new WeakMap<PsmRows, ([number, number] | null)[]>()
  /**
   * The scores (`values`, by SCORES) and the features (`scratch`) of row r of `f`, rows of quantms.psm.parquet or of
   * an input's psm file, which has the same rows.
   */
  const evaluate = (f: PsmRows, r: number): void => {
    let scoreUses = usesOf.get(f)
    if (!scoreUses) usesOf.set(f, (scoreUses = f.scores.names.map(useOf)))
    const observed = f.observedMz[r]
    const calc = f.calculatedMz[r]
    const z = f.charge[r]
    // the scores and the features among them
    values.fill(NaN)
    scratch.fill(NaN)
    for (let k = f.scores.offsets[r]; k < f.scores.offsets[r + 1]; k++) {
      const c = f.scores.name[k]
      const use = c < 0 ? null : scoreUses[c]
      if (use === null) continue
      const v = f.scores.value[k]
      if (use[0] >= 0) values[use[0]] = v
      if (use[1] >= 0 && !Number.isNaN(v)) scratch[use[1]] = use[1] === isotopeFeatureJ ? isotopeSign * v : v
    }
    scratch[0] = values[hyperscoreS]
    // the isotope error of QPX (observed minus theoretical)
    const isotope = first(f.isotope[r], values[isotopeErrorS])

    // the .pin features (PercolatorInfile::stampPinFeaturesOnHits). Percolator saw every hit with its proteins, but a
    // hit whose proteins all failed the protein FDR has none left in QPX (one input: ProSE removes the references):
    // its features are those of the hit alone, without enzN and enzC, which need the residues around the peptide
    // in a protein
    const sequence = String(f.sequence[r] ?? '')
    const e0 = f.evidence.offsets[r]
    // Percolator's dm: corrected by ProSE's isotope error meta value as it is (with its convention's sign)
    const iso = Math.fround(isotopeSign * isotope)
    const dm = (Number.isNaN(iso) ? observed : observed - (iso * C13C12_MASSDIFF_U) / z) - calc
    scratch[calcMassJ] = calc
    scratch[massJ] = observed
    scratch[peplenJ] = sequence.length
    for (let c = 0; c < chargeColumns.length; c++) scratch[chargeColumns[c]] = minCharge + c === z ? 1 : 0
    if (f.evidence.offsets[r + 1] > e0) {
      // a protein terminus ('-' in QPX) as OpenMS' '[' and ']', which Percolator's features were computed with
      const before = f.evidence.aa_before[e0] === '-' ? '[' : String(f.evidence.aa_before[e0] ?? '')
      const after = f.evidence.aa_after[e0] === '-' ? ']' : String(f.evidence.aa_after[e0] ?? '')
      scratch[enzNJ] = isEnz(before.charAt(0), sequence.charAt(0), enzyme) ? 1 : 0
      scratch[enzCJ] = isEnz(sequence.charAt(sequence.length - 1), after.charAt(0), enzyme) ? 1 : 0
    }
    scratch[enzIntJ] = countEnzymatic(sequence, enzyme)
    scratch[dmJ] = dm
    scratch[absdmJ] = Math.abs(dm)
  }

  const peptideRow = new Map<string, number>()
  const pt = {
    openms: [] as string[],
    proforma: [] as string[],
    sequence: [] as string[],
    decoy: [] as number[],
    modOffsets: [0],
    modPosition: [] as number[],
    modName: [] as string[],
    modUnimod: [] as number[],
    modMass: [] as number[],
    proteinOffsets: [0],
    protein: [] as number[],
    start: [] as number[],
    end: [] as number[],
  }
  const peptideBest: number[] = []

  const clampInt8 = (v: number) => (Number.isNaN(v) ? 0 : Math.max(-128, Math.min(127, Math.round(v))))

  for (let k = 0; k < inputs.length; k++) psmFile.fill(k, from[k], from[k + 1])
  // the rows, a batch of row groups at a time: what is read of a batch is let go before the next is read
  for (const batch of batches) {
    const f = await inFile(MERGED_PSMS, async () => readRows(await readColumns(files, MERGED_PSMS, psmMetadata!, batch.groups, ROW_COLUMNS), batch.groups))
    if (f.count !== batch.rows) throw new Error(`${MERGED_PSMS}: ${f.count} rows in the row groups ${batch.groups[0]} to ${batch.groups[batch.groups.length - 1]}, its footer says ${batch.rows}`)
    psmId.set(f.psmId, batch.from)
    for (let r = 0; r < f.count; r++) {
      const i = batch.from + r
      nativeId[i] = String(f.nativeId[r] ?? '')
      rt[i] = f.rt[r]
      const observed = f.observedMz[r]
      const calc = f.calculatedMz[r]
      mz[i] = observed
      const z = f.charge[r]
      charge[i] = clampInt8(z)
      decoy[i] = f.decoy[r] === true ? 1 : 0

      // its scores, and its features: which of them it has, and the rank of a co-isolated precursor
      evaluate(f, r)
      for (let j = 1; j < nFeatures; j++) if (scratch[j] === scratch[j]) featurePresent[j] = 1
      if (rank) rank[i] = scratch[rankJ]
      const hyperscore = values[hyperscoreS]
      const percolator = values[percolatorScoreS]
      const s = Number.isNaN(percolator) ? hyperscore : percolator
      score[i] = s
      qValue[i] = first(values[qS], values[percolatorQS])
      pep[i] = first(f.pep[r], values[percolatorPepS])
      const isotope = first(f.isotope[r], values[isotopeErrorS])
      isotopeError[i] = clampInt8(isotope)
      const corrected = observed - ((Number.isNaN(isotope) ? 0 : isotope) * C13C12_MASSDIFF_U) / z
      precursorErrorPpm[i] = first(values[precursorErrorS], calc > 0 ? ((corrected - calc) / calc) * 1e6 : NaN)
      deltaMass[i] = values[deltaMassS]
      const sequence = String(f.sequence[r] ?? '')
      const e0 = f.evidence.offsets[r]

      // its peptide, unique by the OpenMS string
      const mods = modificationsOf(f.modifications, r, sequence.length)
      const openms = openmsString(sequence, mods)
      let k = peptideRow.get(openms)
      if (k !== undefined) {
        peptide[i] = k
        peptideBest[k] = fmax(peptideBest[k], s)
        continue
      }
      k = pt.openms.length
      peptideRow.set(openms, k)
      peptide[i] = k
      peptideBest.push(s)
      pt.openms.push(openms)
      pt.proforma.push(proformaString(sequence, mods))
      pt.sequence.push(sequence)
      pt.decoy.push(decoy[i])
      for (const m of mods) {
        pt.modPosition.push(m.position)
        pt.modName.push(m.name)
        pt.modUnimod.push(m.unimod)
        pt.modMass.push(m.mass)
      }
      pt.modOffsets.push(pt.modPosition.length)
      // its proteins: the protein mappings of its first PSM whose protein is a row of the proteins
      for (let e = e0; e < f.evidence.offsets[r + 1]; e++) {
        const p = proteinRow.get(String(f.evidence.accession[e]))
        if (p === undefined) continue // a protein the protein FDR removed
        const protein = proteinSequence[p]
        // 1-based first and last position -> 0-based
        let begin = f.evidence.start[e] - 1
        let last = f.evidence.end[e] - 1
        let known = begin >= 0 && last >= begin
        const inSequence =
          known && last < protein.length && last - begin + 1 === sequence.length && protein.startsWith(sequence, begin)
        if (!inSequence) {
          const at = protein.indexOf(sequence)
          if (at >= 0 && sequence.length > 0) {
            begin = at
            last = at + sequence.length - 1
            known = true
          }
        }
        pt.protein.push(p)
        pt.start.push(known ? begin : 0)
        pt.end.push(known ? last + 1 : 0)
      }
      pt.proteinOffsets.push(pt.protein.length)
    }
  }

  // the score of a protein: the best score of its PSMs
  for (let k = 0; k < pt.openms.length; k++) {
    for (let r = pt.proteinOffsets[k]; r < pt.proteinOffsets[k + 1]; r++) {
      proteinScore[pt.protein[r]] = fmax(proteinScore[pt.protein[r]], peptideBest[k])
    }
  }

  // only the features some PSM has
  const kept = featureNames.map((_, j) => j).filter((j) => featurePresent[j])
  /** The features of row r of `f` (see `evaluate`), by the tables' `featureNames`. */
  const featuresOf = (f: PsmRows, r: number): Float64Array => {
    evaluate(f, r)
    return Float64Array.from(kept, (j) => scratch[j])
  }

  const paths = inputs.map((r) => r.path)
  const tables: ResultTables = {
    files: paths,
    psms: {
      count,
      file: psmFile,
      nativeId,
      rt,
      mz,
      charge,
      peptide,
      score,
      qValue,
      pep,
      precursorErrorPpm,
      isotopeError,
      deltaMass,
      decoy,
      featureNames: kept.map((j) => featureNames[j]),
      chimericRank: rank && featurePresent[rankJ] ? rank : null,
    },
    peptides: {
      count: pt.openms.length,
      openms: pt.openms,
      proforma: pt.proforma,
      sequence: pt.sequence,
      decoy: Uint8Array.from(pt.decoy),
      modOffsets: Uint32Array.from(pt.modOffsets),
      modPosition: Int16Array.from(pt.modPosition),
      modName: pt.modName,
      modUnimod: Int32Array.from(pt.modUnimod),
      modMass: Float64Array.from(pt.modMass),
      proteinOffsets: Uint32Array.from(pt.proteinOffsets),
      protein: Uint32Array.from(pt.protein),
      start: Uint32Array.from(pt.start),
      end: Uint32Array.from(pt.end),
    },
    proteins: {
      count: proteinCount,
      accession,
      description,
      sequence: proteinSequence,
      score: proteinScore,
      qValue: Float64Array.from(proteinQ),
      decoy: Uint8Array.from(proteinDecoy),
      group: Int32Array.from(group),
    },
  }

  //------------------------------------------------------------------------------
  // Spectra and predictions: read from the inputs' psm files when a PSM is opened

  const spectra = psmSpectra(
    files,
    inputs.map((input) => input.stem),
    from,
    { nativeId, psmId, precursorMz: mz, rt },
    meta.runs.some((r) => r.prediction_nce !== null),
    featuresOf,
  )

  //------------------------------------------------------------------------------
  // Summary

  let modificationAnalysis: ProSESummary['modificationAnalysis'] = null
  const deltaName = S.get('mod_analysis_delta_masses')
  const ptmName = S.get('mod_analysis_ptms')
  if (parameterFlag(S, 'open_search') && deltaName && ptmName) {
    const [deltas, ptms] = await Promise.all([optional(`mods/${deltaName}`), optional(`mods/${ptmName}`)])
    if (deltas && ptms) {
      modificationAnalysis = await inFile(`mods/{${deltaName},${ptmName}}`, () =>
        parseModificationAnalysis(utf8.decode(deltas), utf8.decode(ptms)),
      )
    }
  }
  // whether some PSM of each input has a q-value: a ProSE that does not report whether it applied the PSM FDR
  // skipped it for an input without
  const hasQValues = inputs.map((_, k) => {
    for (let i = from[k]; i < from[k + 1]; i++) if (!Number.isNaN(qValue[i])) return true
    return false
  })
  const summary = toSummary(meta, paths, modificationAnalysis, hasQValues)
  return {
    summary,
    tables,
    spectra,
    warnings,
    proteinsSaved,
    provenance: Object.fromEntries(meta.search),
    ...(meta.toolVersion ? { toolVersion: meta.toolVersion } : {}),
  }
}

/** The modifications of QPX row `r`, by position (−1 N-terminal, the length C-terminal, else 0-based). */
function modificationsOf(mods: Modifications | null, r: number, length: number): Modification[] {
  if (!mods || mods.offsets[r] === mods.offsets[r + 1]) return []
  const out: Modification[] = []
  for (let m = mods.offsets[r]; m < mods.offsets[r + 1]; m++) {
    const name = mods.name[m] < 0 ? '' : mods.names[mods.name[m]]
    const accession = /^UNIMOD:(\d+)$/i.exec(mods.accession[m] < 0 ? '' : mods.accessions[mods.accession[m]])
    const unimod = accession ? Number(accession[1]) : -1
    const mass = modificationMass(name, unimod)
    for (let k = mods.positionOffsets[m]; k < mods.positionOffsets[m + 1]; k++) {
      const position = mods.position[k]
      out.push({ position: position === 0 ? -1 : position === length + 1 ? length : position - 1, name, unimod, mass })
    }
  }
  return out.sort((a, b) => a.position - b.position)
}
