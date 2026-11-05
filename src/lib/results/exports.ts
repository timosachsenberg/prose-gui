// The exports of the Export menu: the current view as CSV, the design, the search's idXML, the methods section;
// each saved where the user chooses (the system's save dialog).

import { writeDesignTsv } from '../design/design-tsv'
import { saveBytes, saveSearchFile, saveText } from '../desktop'
import { APP_VERSION } from '../history/history'
import type { OpenResult } from '../search/session.svelte'
import { defaultsOf } from '../settings/parameters'
import { PROSE_PARAMETERS } from '../settings/prose'
import { CRAP_PROTEINS } from '../search/form.svelte'
import { toolState } from '../search/tool.svelte'
import { resultName } from './display'
import { methodsPlainText, methodsSection, type MethodsSection } from './methods'
import type { ResultsView } from './view.svelte'

/** The name of a result's exports (see `resultName`). */
export function exportName(result: OpenResult): string {
  return resultName(result.design.rows.map((r) => r.fileName))
}

/**
 * The version of ProSE that searched: as the result's provenance records it, else that of the stored search or of
 * this session's tool; '' if none is known.
 */
export function searchVersion(result: OpenResult): string {
  return result.outcome.toolVersion || [result.stored?.moduleVersion, toolState.info?.version].find((v) => v !== undefined && v !== '' && v !== 'unknown') || ''
}

/** The methods section of a search (`methods.ts`), for a publication. */
export function resultMethods(result: OpenResult): MethodsSection {
  return methodsSection({
    summary: result.outcome.summary,
    settings: result.settings,
    defaults: defaultsOf(PROSE_PARAMETERS),
    provenance: result.outcome.provenance,
    version: searchVersion(result),
    appVersion: result.stored?.appVersion ?? APP_VERSION,
    files: result.design.rows.map((r) => r.fileName),
    instruments: result.instruments,
    contaminantProteins: CRAP_PROTEINS,
    accepted: result.accepted,
  })
}

/** The methods section as `<name>_methods.txt`: the paragraphs and the references, as plain text. */
export async function exportMethods(result: OpenResult): Promise<void> {
  await saveText(`${exportName(result)}_methods.txt`, methodsPlainText(resultMethods(result)) + '\n')
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
