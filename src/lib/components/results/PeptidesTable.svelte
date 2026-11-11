<script lang="ts">
  // The peptides (peptidoforms) of the open result, with PSM counts at the "best PSM q-value" filter. A decoy
  // is marked after its sequence, as the Proteins table marks a decoy protein.
  import PeptideSequence from '$lib/components/PeptideSequence.svelte'
  import { peptideView } from '$lib/format/peptide'
  import { peptideMarks } from '$lib/results/curation'
  import { formatCount, formatQValue } from '$lib/results/display'
  import type { LevelState, ResultsView } from '$lib/results/view.svelte'
  import { tableColumns, sampleIndex, type CellDef } from './cells'
  import DataTable from './DataTable.svelte'
  import Marker from './Marker.svelte'
  import ProteinRefs from './ProteinRefs.svelte'

  // a row opens the peptide's detail beside the table (`onOpen`); while it is open the arrow keys go through the
  // peptides, the detail following; Enter on the open peptide (or a double click) goes on to its PSMs (`onDrill`),
  // Backspace up to its protein (`onBack`); a protein accession opens the protein's panel
  let {
    view,
    level,
    onOpen,
    onOpenProtein,
    onDrill,
    onBack,
  }: {
    view: ResultsView
    level: LevelState
    onOpen: (peptide: number) => void
    onOpenProtein: (protein: number) => void
    onDrill?: (peptide: number) => void
    onBack?: () => void
  } = $props()

  let table = $state<DataTable | null>(null)
  /** Makes the peptide's row the grid's active one and gives the grid the focus. */
  export function focusRow(peptide: number | null): void {
    table?.focusRow(peptide)
  }

  const m = $derived(view.model)
  const pt = $derived(m.tables.peptides)
  const nS = $derived(m.samples.length)
  const nBio = $derived(new Set(m.columnSample).size)
  const fixedMods = $derived(view.result.settings.fixedMods)

  /** The accepted and rejected PSMs of peptide `k` (read with the count of changes: the marks change in place). */
  function marksOf(k: number): { accepted: number; rejected: number } {
    void view.curationVersion
    const marks = peptideMarks(m)
    return { accepted: marks.accepted[k], rejected: marks.rejected[k] }
  }

  function cells(id: string): CellDef {
    const counts = level.counts
    const s = sampleIndex(id)
    if (s >= 0) {
      const count = (k: number) => counts.perSample[k * nS + s]
      return { cell: (k) => (count(k) === 0 ? '–' : formatCount(count(k))), placeholder: (k) => count(k) === 0 }
    }
    switch (id) {
      case 'peptide':
        return { cell: () => peptideCell, cellTitle: (k) => (pt.decoy[k] ? `${pt.proforma[k]} (decoy)` : pt.proforma[k]) }
      case 'proteins':
        return { cell: () => proteinsCell, cellTitle: (k) => Array.from(m.peptide.proteins[k], (p) => m.tables.proteins.accession[p]).join('\n') }
      case 'psms':
        return { cell: (k) => formatCount(counts.psms[k]) }
      case 'foundIn':
        return { cell: (k) => `${counts.foundIn[k]} / ${nBio}`, class: 'text-muted-foreground' }
      case 'bestQ':
        return { cell: (k) => formatQValue(m.peptide.bestQ[k]) }
      case 'length':
        return { cell: (k) => String(pt.sequence[k].length) }
      case 'curation':
        return {
          cell: () => curationCell,
          cellTitle: (k) => {
            const { accepted, rejected } = marksOf(k)
            return accepted + rejected === 0 ? 'No curated PSM' : `${accepted} accepted, ${rejected} rejected PSMs`
          },
        }
      default:
        return { cell: () => '' }
    }
  }

  const columns = $derived(tableColumns(level.visible, cells))
</script>

{#snippet curationCell(k: number)}
  {@const marks = marksOf(k)}
  {#if marks.accepted > 0}<span class="text-emerald-700 tabular-nums dark:text-emerald-400"
      ><span aria-hidden="true">✓</span>{marks.accepted}<span class="sr-only"> accepted</span></span
    >{/if}{#if marks.rejected > 0}<span class="ml-2 text-destructive tabular-nums"
      ><span aria-hidden="true">✗</span>{marks.rejected}<span class="sr-only"> rejected</span></span
    >{/if}
{/snippet}

{#snippet peptideCell(k: number)}
  <PeptideSequence view={peptideView(pt, k, fixedMods)} />{#if pt.decoy[k]}<Marker text="decoy" />{/if}
{/snippet}

{#snippet proteinsCell(k: number)}
  <ProteinRefs model={m} peptide={k} onOpen={onOpenProtein} />
{/snippet}

<DataTable
  bind:this={table}
  label="Peptides"
  rows={level.rows}
  {columns}
  sort={level.effectiveSort}
  onSort={(key) => level.sortBy(key)}
  onRowClick={onOpen}
  onRowOpen={onDrill}
  {onBack}
  followFocus={view.peptidePanel !== null}
  selected={level.selected}
  empty={level.emptyText} />
