// The exports of the Export menu: the current view as CSV, the design, the search's idXML; each saved
// where the user chooses (the system's save dialog).

import { writeDesignTsv } from '../design/design-tsv'
import { saveBytes, saveSearchFile, saveText } from '../desktop'
import { APP_VERSION } from '../history/history'
import type { OpenResult } from '../search/session.svelte'
import { defaultsOf } from '../settings/parameters'
import { PROSE_PARAMETERS } from '../settings/prose'
import { resultName } from './display'
import type { ResultsView } from './view.svelte'

/** The name of a result's exports (see `resultName`). */
export function exportName(result: OpenResult): string {
  return resultName(result.design.rows.map((r) => r.fileName))
}

/** After the webview has painted (so that "Exporting…" shows before a long export). */
const painted = () => new Promise<void>((resolve) => requestAnimationFrame(() => setTimeout(resolve, 0)))

/**
 * The current level's view as `<name>-<level>.csv` (`view.exporting` meanwhile); false on a tab
 * without a table or while an export runs.
 */
export async function exportCsv(view: ResultsView): Promise<boolean> {
  const level = view.level
  if (!level || view.exporting) return false
  view.exporting = true
  try {
    await painted()
    await saveText(`${exportName(view.result)}-${level.level}.csv`, level.csv())
    return true
  } finally {
    view.exporting = false
  }
}

/** The table view (proteins or peptides; PSMs too) as `<name>-<level>.xlsx`, the rows and columns of the CSV. */
export async function exportXlsx(view: ResultsView): Promise<boolean> {
  const level = view.level
  if (!level || view.exporting) return false
  view.exporting = true
  try {
    await painted()
    await saveBytes(`${exportName(view.result)}-${level.level}.xlsx`, level.xlsx())
    return true
  } finally {
    view.exporting = false
  }
}

/** The design of the searched files as `<name>_design.tsv` (OpenMS' one-table format). */
export async function exportDesign(result: OpenResult): Promise<void> {
  await saveText(`${exportName(result)}_design.tsv`, writeDesignTsv(result.design))
}

/**
 * The idXML of a search in its folder (src-tauri/src/args.rs): the merged one of several files
 * (`-out_merged`), the file's own of one (`-out`; a search of one file saved by an earlier build has
 * none, and the export says so).
 */
export function idXMLFile(result: OpenResult): { name: string; label: string } {
  return result.design.rows.length > 1 ? { name: 'merged.idXML', label: 'idXML (merged)' } : { name: 'search.idXML', label: 'idXML' }
}

/** The search's idXML, saved as `<name>.idXML`. */
export async function exportIdXML(result: OpenResult): Promise<void> {
  await saveSearchFile(result.id, idXMLFile(result).name, `${exportName(result)}.idXML`)
}
