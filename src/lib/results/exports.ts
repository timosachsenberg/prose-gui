// The exports of the Export menu: the current view as CSV or TSV, the curated PSMs as TSV, the design, the
// search's idXML; each saved where the user chooses (the system's save dialog).

import { writeDesignTsv } from '../design/design-tsv'
import { saveSearchFile } from '../desktop'
import { download } from '../download'
import type { OpenResult } from '../search/session.svelte'
import { toTsv } from './csv'
import { resultName } from './display'
import { csvColumns } from './table-columns'
import type { ResultsView } from './view.svelte'

/** The name of a result's exports (see `resultName`). */
export function exportName(result: OpenResult): string {
  return resultName(result.design.rows.map((r) => r.fileName))
}

/** After the webview has painted (so that "Exporting…" shows before a long export). */
const painted = () => new Promise<void>((resolve) => requestAnimationFrame(() => setTimeout(resolve, 0)))

/**
 * The current level's view as `<name>-<level>.csv` or `.tsv` (`view.exporting` meanwhile); false on a tab
 * without a table or while an export runs.
 */
export async function exportCsv(view: ResultsView, format: 'csv' | 'tsv' = 'csv'): Promise<boolean> {
  const level = view.level
  if (!level || view.exporting) return false
  view.exporting = true
  try {
    await painted()
    const [text, type] = format === 'csv' ? [level.csv(), 'text/csv;charset=utf-8'] : [level.tsv(), 'text/tab-separated-values']
    await download(`${exportName(view.result)}-${level.level}.${format}`, text, type)
    return true
  } finally {
    view.exporting = false
  }
}

/**
 * The curated PSMs as `<name>-curated-psms.tsv`: every PSM with a mark or a note, whatever the PSM table's filters,
 * in its order, with its visible columns and the Curation and Curation note columns (also when hidden). False when no
 * PSM has either or while an export runs.
 */
export async function exportCurated(view: ResultsView): Promise<boolean> {
  if (view.exporting) return false
  const rows = view.curatedRows()
  if (rows.length === 0) return false
  view.exporting = true
  try {
    await painted()
    const columns = csvColumns(view.psms.visible)
    for (const id of ['curation', 'curationNote']) if (!columns.includes(id)) columns.push(id)
    const text = toTsv(view.model, 'psms', rows, columns, view.psms.filters)
    await download(`${exportName(view.result)}-curated-psms.tsv`, text, 'text/tab-separated-values')
    return true
  } finally {
    view.exporting = false
  }
}

/** The design of the searched files as `<name>_design.tsv` (OpenMS' one-table format). */
export async function exportDesign(result: OpenResult): Promise<void> {
  await download(`${exportName(result)}_design.tsv`, writeDesignTsv(result.design), 'text/tab-separated-values')
}

/**
 * The idXML of a search in its folder (src-tauri/src/args.rs): the merged one of several files
 * (`-out_merged`), the file's own of one (`-out_idxml`; a search of one file saved by an earlier build has
 * none, and the export says so).
 */
export function idXMLFile(result: OpenResult): { name: string; label: string } {
  return result.design.rows.length > 1 ? { name: 'merged.idXML', label: 'idXML (merged)' } : { name: 'search.idXML', label: 'idXML' }
}

/** The search's idXML, saved as `<name>.idXML`. */
export async function exportIdXML(result: OpenResult): Promise<void> {
  await saveSearchFile(result.id, idXMLFile(result).name, `${exportName(result)}.idXML`)
}
