// A search folder as the results reader reads it: its files through the `files` callback (`SearchFiles`; the
// contract is in read.ts), the names of the QPX views, and the PSM rows of a psm file (quantms.psm.parquet, or
// an input's own in qpx/runs/), which the tables (read.ts) and the spectra of a PSM (psm-spectra.ts) are made of.

import { errorText } from '../format/error'
import { readLeaf, readStructList, type ParquetFile } from './parquet'

/** A part of a file: its last `tail` bytes (all of a smaller file), or `length` bytes from `offset` (fewer at its end). */
export type ByteRange = { tail: number } | { offset: number; length: number }

/**
 * The contents of a file of the search folder by its name relative to the folder (or an absolute path): all
 * of it, or the bytes of `range`. A file that does not exist: reject with FileNotFound (or anything with
 * `notFound: true`).
 */
export type SearchFiles = (name: string, range?: ByteRange) => Promise<Uint8Array>

/** The bytes of `range` of a file's contents `bytes` (for a `SearchFiles` that has the whole file at hand). */
export function bytesOfRange(bytes: Uint8Array, range?: ByteRange): Uint8Array {
  if (!range) return bytes
  if ('tail' in range) return bytes.subarray(Math.max(0, bytes.length - range.tail))
  const start = Math.min(range.offset, bytes.length)
  return bytes.subarray(start, Math.min(bytes.length, start + range.length))
}

/** The rejection of `SearchFiles` for a file that does not exist. */
export class FileNotFound extends Error {
  readonly notFound = true
  constructor(name: string) {
    super(`${name} does not exist.`)
    this.name = 'FileNotFound'
  }
}

/** Whether `files` rejected because the file does not exist. */
export function isNotFound(e: unknown): boolean {
  return typeof e === 'object' && e !== null && (e as { notFound?: unknown }).notFound === true
}

/** Runs `read` on the file `name`; an error names the file. */
export async function inFile<T>(name: string, read: () => T | Promise<T>): Promise<T> {
  try {
    return await read()
  } catch (e) {
    const message = errorText(e)
    if (message.startsWith(`${name}:`)) throw e
    throw new Error(`${name}: ${message}`, { cause: e })
  }
}

/** The QPX views of the search (in qpx/). */
export const MERGED_PSMS = 'qpx/quantms.psm.parquet'
export const RUNS = 'qpx/quantms.run.parquet'
export const PROVENANCE = 'qpx/quantms.provenance.parquet'
export const GROUPS = 'qpx/quantms.pg.parquet'

/** The columns -qpx_extended adds to every QPX psm view (besides the scores). */
export const EXTENDED_COLUMNS = ['spectrum_reference', 'isotope_error', 'protein_mappings']

/** The first of `values` that is a number (not NaN); NaN if none is. */
export const first = (...values: number[]) => values.find((v) => !Number.isNaN(v)) ?? NaN

//------------------------------------------------------------------------------
// The PSM rows

/** The PSM rows of the search: the columns of quantms.psm.parquet. */
export interface PsmRows {
  count: number
  sequence: unknown[]
  charge: Float64Array
  pep: Float64Array
  decoy: unknown[]
  /** the scores: names as codes into `names` */
  scores: { offsets: Uint32Array; name: Int32Array; names: string[]; value: Float64Array }
  /** QPX modifications, as leaves: null if no row has one */
  modifications: Modifications | null
  /** QPX psm_id (as doubles): the identity of a row in the input's own psm file */
  psmId: Float64Array
  nativeId: unknown[]
  rt: ArrayLike<number>
  observedMz: ArrayLike<number>
  calculatedMz: ArrayLike<number>
  /** QPX isotope_error (observed minus theoretical); NaN: null */
  isotope: Float64Array
  /** protein_mappings: 1-based start and end, NaN: null */
  evidence: { offsets: Uint32Array; accession: unknown[]; aa_before: unknown[]; aa_after: unknown[]; start: Float64Array; end: Float64Array }
}

/** The QPX modifications of the rows: row r has the modifications offsets[r] .. offsets[r + 1], modification m the
 * positions positionOffsets[m] .. positionOffsets[m + 1] */
export interface Modifications {
  offsets: Uint32Array
  name: Int32Array
  names: string[]
  accession: Int32Array
  accessions: string[]
  positionOffsets: Uint32Array
  position: Float64Array
}

/** The columns of quantms.psm.parquet that the PSM rows are read from. */
export const ROW_COLUMNS = [
  'sequence',
  'charge',
  'posterior_error_probability',
  'is_decoy',
  'additional_scores',
  'modifications',
  'psm_id',
  'rt',
  'observed_mz',
  'calculated_mz',
  ...EXTENDED_COLUMNS,
]

/** The PSM rows of the row groups `groups` of a psm file (whose ROW_COLUMNS of these groups are read). */
export function readRows(qpx: ParquetFile, groups: number[]): PsmRows {
  const flat = <T>(column: string) => readLeaf(qpx, [column], 'values', groups).values as T
  const scores = readStructList(qpx, 'additional_scores', ['score_name', 'score_value'], { score_name: 'codes' }, groups)
  const evidence = readStructList(qpx, 'protein_mappings', ['accession', 'aa_before', 'aa_after', 'start', 'end'], {}, groups)
  // the modifications (a list of structs with a list): as leaves, without an object per row; only if some row has any
  const modLeaf = readLeaf(qpx, ['modifications', 'list', 'element', 'name'], 'codes', groups)
  const count = modLeaf.rows
  const modified = modLeaf.offsets![count] > 0
  let modifications: Modifications | null = null
  if (modified) {
    const accession = readLeaf(qpx, ['modifications', 'list', 'element', 'accession'], 'codes', groups)
    const position = readLeaf(qpx, ['modifications', 'list', 'element', 'positions', 'list', 'element', 'position'], 'values', groups)
    modifications = {
      offsets: modLeaf.offsets!,
      name: modLeaf.values as Int32Array,
      names: modLeaf.dictionary ?? [],
      accession: accession.values as Int32Array,
      accessions: accession.dictionary ?? [],
      positionOffsets: position.offsets2!,
      position: position.values as Float64Array,
    }
    if (position.offsets![count] !== modLeaf.offsets![count]) throw new Error('the modifications and their positions do not agree')
  }
  return {
    count,
    sequence: flat<unknown[]>('sequence'),
    charge: flat<Float64Array>('charge'),
    pep: flat<Float64Array>('posterior_error_probability'),
    decoy: flat<unknown[]>('is_decoy'),
    scores: {
      offsets: scores.offsets,
      name: scores.score_name as Int32Array,
      names: scores.dictionaries.score_name ?? [],
      value: scores.score_value as Float64Array,
    },
    modifications,
    psmId: flat<Float64Array>('psm_id'),
    nativeId: flat<unknown[]>('spectrum_reference'),
    rt: flat<Float32Array>('rt'),
    observedMz: flat<Float32Array>('observed_mz'),
    calculatedMz: flat<Float32Array>('calculated_mz'),
    isotope: flat<Float64Array>('isotope_error'),
    evidence: {
      offsets: evidence.offsets,
      accession: evidence.accession as unknown[],
      aa_before: evidence.aa_before as unknown[],
      aa_after: evidence.aa_after as unknown[],
      start: evidence.start as Float64Array,
      end: evidence.end as Float64Array,
    },
  }
}
