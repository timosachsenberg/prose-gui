// The spectra of the PSMs, read when a PSM is opened: by row group of the inputs' psm files (PeptDeep's
// prediction, the features) and mz files (the peaks), the last few row groups kept decoded.

import type { FileMetaData } from 'hyparquet'
import { webIonName } from './derive'
import { chunkRange, metadataOfTail, partOfParquet, readLeaf, type LeafMode, type ParquetFile } from './parquet'
import { first, inFile, isNotFound, MERGED_PSMS, readRows, ROW_COLUMNS, type PsmRows, type SearchFiles } from './search-folder'
import type { PredictedSpectra, StoredSpectra } from './types'

/** The peak filter of the stored spectra: the `top` most intense peaks per `window` Th, and the annotated ones. */
export interface PeakFilter {
  top: number
  window: number
}

/**
 * The peaks (the input's mz view) and PeptDeep's prediction (its psm file) of the PSMs of a row group, as the web
 * app's StoredSpectra and PredictedSpectra of those rows.
 */
export interface LoadedSpectra {
  /** the spectra of a row group of the input's mz view; the PSM's is spectrumRow (−1: none) */
  spectra: StoredSpectra
  spectrumRow: number
  /** the predictions of a row group of the input's psm file; the PSM's is row */
  predicted: PredictedSpectra
  row: number
  /** the peak filter ProSE applied to the input's spectra (the PSM view says so; its mz view's footer); null: every peak */
  peakFilter: PeakFilter | null
  /**
   * The PSM's features, by ResultTables.psms.featureNames (NaN: absent): Percolator's input features as ProSE's
   * rescoring gives them, read from the PSM's row of its input's psm file (the tables do not keep them).
   */
  features: Float64Array
}

/** The peaks and predictions of the PSM rows, read when a PSM is opened. */
export interface PsmSpectra {
  /** ProSE predicted spectra (it has ONNX Runtime): some input has the NCE of a prediction */
  predictions: boolean
  /** The peaks, the prediction and the features of PSM row `row` (of ResultTables.psms), read from its input's files. */
  load(row: number): Promise<LoadedSpectra>
}

/** The columns of the predictions, in the per-input psm files only. */
const PREDICTION_COLUMNS = ['predicted_ion_type_array', 'predicted_mz_array', 'predicted_intensity_array']
/** The columns of the mz view the PSM view reads. */
const PEAK_COLUMNS = ['id', 'mz', 'intensity']

interface ListLeaf<T> {
  offsets: Uint32Array
  nullList: Uint8Array
  values: T
}

function listLeaf<T>(file: ParquetFile, column: string, mode: LeafMode = 'values', groups?: number[]): ListLeaf<T> & { dictionary?: string[] } {
  const leaf = readLeaf(file, [column, 'list', 'element'], mode, groups)
  return { offsets: leaf.offsets!, nullList: leaf.nullList!, values: leaf.values as T, dictionary: leaf.dictionary }
}

/** The row groups of an input's files that the PSM view keeps decoded (the last ones opened). */
const CACHED_GROUPS = 4

/** The PSM rows' values that the spectra are checked against. */
interface RowIdentity {
  /** the native ID of each PSM row (the mz view's `id` of its spectrum) */
  nativeId: string[]
  /** the psm_id of each PSM row (in the input's psm file the same) */
  psmId: Float64Array
  /** the precursor m/z of each PSM row (the spectrum's, when the mz view has none) */
  precursorMz: Float64Array
  /** the RT of each PSM row in seconds (NaN: unknown): its spectrum's scan_start_time */
  rt: Float64Array
}

/** A file's metadata and where its rows are: row group g has the rows starts[g] .. starts[g + 1]. */
interface Layout {
  metadata: FileMetaData
  starts: number[]
}

/** The row group of `row` and its row in it. */
function groupOf(layout: Layout, row: number): [number, number] {
  let g = 0
  while (g + 1 < layout.starts.length - 1 && row >= layout.starts[g + 1]) g++
  return [g, row - layout.starts[g]]
}

/**
 * The PsmSpectra of the PSM rows: the rows of input k are from[k] .. from[k + 1] of the PSM rows and the rows of
 * its psm file `qpx/runs/<stems[k]>.psm.parquet`, in order; its spectra are the rows of `qpx/runs/<stems[k]>.mz.parquet`,
 * one per spectrum in the order of its first PSM (else found by their ids). A search of app 0.1.x has these files next
 * to quantms.* (`qpx/<stems[k]>.*`): decided per input by its psm file, and used for its mz file too.
 */
export function psmSpectra(
  files: SearchFiles,
  stems: string[],
  from: number[],
  rows: RowIdentity,
  predictions: boolean,
  featuresOf: (rows: PsmRows, row: number) => Float64Array,
): PsmSpectra {
  const layouts = new Map<string, Promise<Layout>>()
  const predictionCache = new Map<string, Promise<{ psmId: Float64Array; predicted: PredictedSpectra; rows: PsmRows }>>()
  const peakCache = new Map<string, Promise<{ id: unknown[]; startTime: Float32Array; spectra: StoredSpectra }>>()
  /** the inputs whose mz rows are found by their ids (the order of the PSMs did not give them) */
  const byIds = new Set<number>()
  const mzRows = new Map<number, Promise<Int32Array>>()
  /** The folder of input k's files ('qpx/runs/' or, of app 0.1.x, 'qpx/'), once `folderOf(k)` has resolved. */
  const folders = new Map<number, string>()
  const folderPromises = new Map<number, Promise<string>>()
  const psmName = (k: number) => `${folders.get(k) ?? 'qpx/runs/'}${stems[k]}.psm.parquet`
  const mzName = (k: number) => `${folders.get(k) ?? 'qpx/runs/'}${stems[k]}.mz.parquet`

  /** A value made once, kept until it fails. */
  const once = <K, T>(map: Map<K, Promise<T>>, key: K, make: () => Promise<T>): Promise<T> => {
    let p = map.get(key)
    if (!p) {
      p = make()
      p.catch(() => map.delete(key))
      map.set(key, p)
    }
    return p
  }

  /** The layout of the file `name` (its footer, read once). */
  const layoutOf = (name: string) =>
    once(layouts, name, () =>
      inFile(name, async () => {
        let metadata = metadataOfTail(await files(name, { tail: 1 << 16 }))
        if (typeof metadata === 'number') metadata = metadataOfTail(await files(name, { tail: metadata }))
        if (typeof metadata === 'number') throw new Error('its footer cannot be read')
        const starts = [0]
        for (const g of metadata.row_groups) starts.push(starts[starts.length - 1] + Number(g.num_rows))
        return { metadata, starts }
      }),
    )

  /**
   * Where input k's files are: qpx/runs/ if its psm file is there, else (app 0.1.x) qpx/. Decided once, from the tail
   * of the psm file (read anyway); if neither exists, the runs/ name is the one reported.
   */
  const folderOf = (k: number) =>
    once(folderPromises, k, async () => {
      const notFound = (e: unknown) => isNotFound((e as { cause?: unknown }).cause ?? e)
      const psm = (folder: string) => `${folder}${stems[k]}.psm.parquet`
      let folder = 'qpx/runs/'
      try {
        await layoutOf(psm(folder))
      } catch (e) {
        if (!notFound(e)) throw e
        try {
          await layoutOf(psm('qpx/'))
          folder = 'qpx/'
        } catch (old) {
          if (!notFound(old)) throw old
          throw e
        }
      }
      folders.set(k, folder)
      return folder
    })

  /** The columns `columns` of row group g of the file `name` (one byte range for all of them). */
  const groupFile = async (name: string, layout: Layout, g: number, columns: string[]) => {
    const ranges = layout.metadata.row_groups[g].columns
      .filter((c) => c.meta_data && columns.includes(c.meta_data.path_in_schema[0]))
      .map((c) => chunkRange(c.meta_data!))
    const present = new Set(layout.metadata.row_groups[g].columns.map((c) => c.meta_data?.path_in_schema[0]))
    const missing = columns.filter((c) => !present.has(c))
    if (missing.length > 0) throw new Error(`it has not the columns of ProSE's -qpx_extended (${missing.join(', ')})`)
    const start = Math.min(...ranges.map((r) => r.start))
    const end = Math.max(...ranges.map((r) => r.end))
    const bytes = await files(name, { offset: start, length: end - start })
    if (bytes.byteLength !== end - start) throw new Error(`${bytes.byteLength} bytes at ${start}, expected ${end - start}`)
    return partOfParquet(layout.metadata, bytes, start)
  }

  /**
   * The predictions of row group g of input k's psm file, its psm_id, and its rows as the PSM rows are read
   * (ROW_COLUMNS: for the features of a PSM), from one read of the row group's columns.
   */
  const predictionsOf = (k: number, layout: Layout, g: number) =>
    once(predictionCache, `${k}/${g}`, () =>
      inFile(psmName(k), async () => {
        const file = await groupFile(psmName(k), layout, g, [...ROW_COLUMNS, ...PREDICTION_COLUMNS])
        const groups = [g]
        const rows = readRows(file, groups)
        const psmId = readLeaf(file, ['psm_id'], 'values', groups).values as Float64Array
        const ion = listLeaf<Int32Array>(file, 'predicted_ion_type_array', 'codes', groups)
        const ionNames = (ion.dictionary ?? []).map(webIonName)
        const predictedMz = listLeaf<Float32Array>(file, 'predicted_mz_array', 'values', groups)
        const predictedIntensity = listLeaf<Float32Array>(file, 'predicted_intensity_array', 'values', groups)
        const ions = Array.from(ion.values, (c) => (c < 0 ? '' : ionNames[c]))
        if (predictedMz.values.length !== ions.length || predictedIntensity.values.length !== ions.length) {
          throw new Error(`row group ${g} has other numbers of predicted ions, m/z and intensities`)
        }
        // (a row without a prediction has an empty range: its list is null)
        const predicted: PredictedSpectra = { psmOffsets: ion.offsets, ion: ions, mz: predictedMz.values, intensity: predictedIntensity.values }
        return { psmId, predicted, rows }
      }),
    )

  /** The spectra of row group g of input k's mz view: StoredSpectra of its rows (spectrum s = row s), their ids. */
  const peaksOf = (k: number, layout: Layout, g: number) =>
    once(peakCache, `${k}/${g}`, () =>
      inFile(mzName(k), async () => {
        const file = await groupFile(mzName(k), layout, g, [...PEAK_COLUMNS, 'scan_start_time', 'precursors'])
        const groups = [g]
        const id = readLeaf(file, ['id'], 'values', groups).values as unknown[]
        const startTime = readLeaf(file, ['scan_start_time'], 'values', groups).values as Float32Array
        const peakMz = listLeaf<Float32Array>(file, 'mz', 'values', groups)
        const peakIntensity = listLeaf<Float32Array>(file, 'intensity', 'values', groups)
        const precursor = readLeaf(file, ['precursors', 'list', 'element', 'selected_ion_mz'], 'values', groups)
        const n = id.length
        const precursorMz = new Float64Array(n).fill(NaN)
        for (let r = 0; r < n; r++) {
          if (peakIntensity.offsets[r] !== peakMz.offsets[r] || peakIntensity.offsets[r + 1] !== peakMz.offsets[r + 1]) {
            throw new Error(`row ${r} of row group ${g} has other numbers of peak m/z and intensities`)
          }
          if (precursor.offsets![r + 1] > precursor.offsets![r]) precursorMz[r] = (precursor.values as Float32Array)[precursor.offsets![r]]
        }
        const spectra: StoredSpectra = {
          psmSpectrum: Int32Array.from({ length: n }, (_, r) => r),
          offsets: peakMz.offsets,
          mz: peakMz.values,
          intensity: peakIntensity.values,
          precursorMz,
          // not read: the web app took it as a fallback NCE for its own predictions; these are ProSE's
          collisionEnergy: new Float32Array(n).fill(NaN),
        }
        return { id, startTime, spectra }
      }),
    )

  /**
   * The mz row of each PSM row of input k (−1: none): the order of each spectrum's first PSM; if the mz view has another
   * number of rows (a PSM's spectrum not found), or that order turned out wrong (`byIds`), by the ids of all its rows.
   */
  const mzRowsOf = (k: number, layout: Layout) =>
    once(mzRows, k, async () => {
      const n = from[k + 1] - from[k]
      const out = new Int32Array(n).fill(-1)
      const first = new Map<string, number>()
      for (let r = 0; r < n; r++) {
        const id = rows.nativeId[from[k] + r]
        // no native ID (QPX spectrum_reference null): ProSE stored no spectrum for the PSM, and it has no mz row
        if (id === '') continue
        let m = first.get(id)
        if (m === undefined) {
          m = first.size
          first.set(id, m)
        }
        out[r] = m
      }
      const total = layout.starts[layout.starts.length - 1]
      if (total === first.size && !byIds.has(k)) return out
      // by the ids
      const rowOf = new Map<string, number>()
      for (let g = 0; g < layout.metadata.row_groups.length; g++) {
        const file = await inFile(mzName(k), () => groupFile(mzName(k), layout, g, ['id']))
        const ids = readLeaf(file, ['id'], 'values', [g]).values as unknown[]
        ids.forEach((id, r) => rowOf.set(String(id), layout.starts[g] + r))
      }
      for (let r = 0; r < n; r++) {
        const id = rows.nativeId[from[k] + r]
        out[r] = id === '' ? -1 : (rowOf.get(id) ?? -1)
      }
      return out
    })

  /** The peak filter of input k's mz view (its footer). */
  const filterOf = (layout: Layout): PeakFilter | null => {
    const footer = new Map((layout.metadata.key_value_metadata ?? []).map((kv) => [kv.key, kv.value ?? '']))
    const top = Number(footer.get('prose_peak_top') ?? 0)
    return top > 0 ? { top, window: Number(footer.get('prose_peak_window') ?? 0) } : null
  }

  /** Keeps the last CACHED_GROUPS row groups of each kind. */
  const trim = () => {
    for (const cache of [predictionCache, peakCache] as Map<string, unknown>[]) {
      const keys = [...cache.keys()]
      for (const key of keys.slice(0, Math.max(0, keys.length - CACHED_GROUPS))) cache.delete(key)
    }
  }
  /** Marks `key` of `cache` as used last. */
  const touch = <T>(cache: Map<string, T>, key: string) => {
    const p = cache.get(key)
    if (p) {
      cache.delete(key)
      cache.set(key, p)
    }
  }

  const noSpectra: StoredSpectra = {
    psmSpectrum: new Int32Array(0),
    offsets: new Uint32Array(1),
    mz: new Float32Array(0),
    intensity: new Float32Array(0),
    precursorMz: new Float64Array(0),
    collisionEnergy: new Float32Array(0),
  }

  return {
    predictions,
    async load(row: number): Promise<LoadedSpectra> {
      let k = 0
      while (k + 1 < from.length && row >= from[k + 1]) k++
      if (!(row >= 0) || k >= stems.length) throw new Error(`There is no PSM row ${row}.`)
      const local = row - from[k]

      // the prediction: the same row of the input's psm file (whose psm_id has to be the row's)
      await folderOf(k)
      const psmLayout = await layoutOf(psmName(k))
      if (psmLayout.starts[psmLayout.starts.length - 1] !== from[k + 1] - from[k]) {
        throw new Error(`${psmName(k)}: ${psmLayout.starts[psmLayout.starts.length - 1]} rows, but ${MERGED_PSMS} has ${from[k + 1] - from[k]} of this input`)
      }
      const [g, at] = groupOf(psmLayout, local)
      touch(predictionCache, `${k}/${g}`)
      const { psmId, predicted, rows: groupRows } = await predictionsOf(k, psmLayout, g)
      if (psmId[at] !== rows.psmId[row]) throw new Error(`${psmName(k)}: its row ${local} is another PSM than the search's (psm_id)`)
      // its features: the same row, as the PSM rows are read
      const features = featuresOf(groupRows, at)

      // the spectrum: its row in the input's mz view (whose id has to be the PSM's native ID); none without the view
      let mzLayout: Layout | null = null
      try {
        mzLayout = await layoutOf(mzName(k))
      } catch (e) {
        if (!isNotFound((e as { cause?: unknown }).cause ?? e)) throw e
      }
      let spectra = noSpectra
      let spectrumRow = -1
      let peakFilter: PeakFilter | null = null
      /** The mz row of the PSM and its row group, checked against its native ID (null: none). */
      const spectrumOf = async (layout: Layout) => {
        const m = (await mzRowsOf(k, layout))[local]
        if (m < 0) return null
        const [mg, mat] = groupOf(layout, m)
        touch(peakCache, `${k}/${mg}`)
        const peaks = await peaksOf(k, layout, mg)
        return { m, mat, peaks, matches: String(peaks.id[mat]) === rows.nativeId[row] }
      }
      // (another load of this input may switch it to its ids meanwhile: one that began before must retry too)
      const byIdsBefore = byIds.has(k)
      let found = mzLayout ? await spectrumOf(mzLayout) : null
      if (mzLayout && found && !found.matches && !byIdsBefore) {
        // the order of the PSMs did not give the rows (spectra missing and others duplicated): by their ids, once
        if (!byIds.has(k)) {
          byIds.add(k)
          mzRows.delete(k)
        }
        found = await spectrumOf(mzLayout)
      }
      if (found && !found.matches) {
        throw new Error(`${mzName(k)}: its row ${found.m} is the spectrum ${String(found.peaks.id[found.mat])}, not the PSM's ${rows.nativeId[row]}`)
      }
      // the row of its native ID is another spectrum's (of several spectra of one native ID, the view has the first's):
      // the PSM has none
      const own =
        found !== null && (Number.isNaN(rows.rt[row]) || !(Math.abs(found.peaks.startTime[found.mat] * 60 - rows.rt[row]) > 0.05))
      if (mzLayout && found && own) {
        spectra = found.peaks.spectra
        spectrumRow = found.mat
        if (Number.isNaN(spectra.precursorMz[found.mat])) spectra.precursorMz[found.mat] = rows.precursorMz[row]
        peakFilter = filterOf(mzLayout)
      }
      trim()
      return { spectra, spectrumRow, predicted, row: at, peakFilter, features }
    },
  }
}
