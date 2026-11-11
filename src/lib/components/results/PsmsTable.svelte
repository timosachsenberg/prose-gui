<script lang="ts">
  // The PSMs of the open result (the best hit of each spectrum). An open search's mass shift above
  // 0.05 Da goes before the sequence as an unlocalized ProForma mass ("[+15.9949]?"). A row opens the
  // PSM view (`onOpen`); while it is open, the table stands above it, a few rows high (`follow`: the arrow keys
  // go through the PSMs, the view showing each). `compact` makes it a narrow list (peptide, charge, sample,
  // q-value and precursor error). A decoy PSM and that of a co-isolated
  // precursor (a search with chimeric:enabled) are marked after the peptide. The Curation column shows the user's
  // mark (accepted ✓, rejected ✗; the compact list too).
  import PeptideSequence from '$lib/components/PeptideSequence.svelte'
  import { massShift, peptideView } from '$lib/format/peptide'
  import { COISOLATED_TITLE, isCoisolated } from '$lib/results/coisolated'
  import { ACCEPTED, MARK_NAMES, REJECTED } from '$lib/results/curation'
  import { baseName } from '$lib/results/model'
  import { formatQCompact, formatQValue, formatScore, spectrumLabel } from '$lib/results/display'
  import type { LevelState, ResultsView } from '$lib/results/view.svelte'
  import { fixed, signed, tableColumns, type CellDef } from './cells'
  import DataTable from './DataTable.svelte'
  import Marker from './Marker.svelte'
  import ProteinRefs from './ProteinRefs.svelte'

  let {
    view,
    level,
    onOpen,
    compact = false,
    follow = false,
    onOpenProtein,
    onBack,
  }: {
    view: ResultsView
    level: LevelState
    onOpen: (psm: number) => void
    compact?: boolean
    follow?: boolean
    onOpenProtein?: (protein: number) => void
    /** Backspace: up to the peptide of the PSM. */
    onBack?: (row: number | null) => void
  } = $props()

  const m = $derived(view.model)
  const ps = $derived(m.tables.psms)
  const fixedMods = $derived(view.result.settings.fixedMods)

  /** The peptide's tooltip, with what its markers say (they may be cut off in a narrow column). */
  function withMarks(text: string, i: number): string {
    const marks = [ps.decoy[i] ? 'decoy' : null, isCoisolated(ps, i) ? 'co-isolated precursor' : null].filter((x) => x !== null)
    return marks.length > 0 ? `${text} (${marks.join(', ')})` : text
  }

  /** The PSM's curation mark (read with the count of changes: the marks change in place). */
  function markOf(i: number): number {
    void view.curationVersion
    return m.curation[i]
  }

  /** The PSM's note ('' none; read with the count of changes, as the marks). */
  function noteOf(i: number): string {
    void view.curationVersion
    return m.curationNotes.get(i) ?? ''
  }

  function cells(id: string): CellDef {
    switch (id) {
      case 'curation':
        return { cell: () => curationCell, cellTitle: (i) => MARK_NAMES[markOf(i)] || 'Not curated' }
      case 'note':
        return { cell: (i) => noteOf(i), cellTitle: (i) => noteOf(i) || undefined }
      case 'peptide':
        return { cell: () => peptideCell, cellTitle: (i) => withMarks(m.tables.peptides.proforma[ps.peptide[i]], i) }
      case 'proteins':
        return {
          cell: () => proteinsCell,
          cellTitle: (i) => Array.from(m.peptide.proteins[ps.peptide[i]], (p) => m.tables.proteins.accession[p]).join('\n'),
        }
      case 'sample':
        return { cell: (i) => m.samples[m.psmSample[i]].label, cellTitle: (i) => m.samples[m.psmSample[i]].label }
      case 'scan':
        return { cell: (i) => spectrumLabel(ps.nativeId[i]), cellTitle: (i) => ps.nativeId[i] }
      case 'charge':
        return { cell: (i) => String(ps.charge[i]) }
      case 'mz':
        return { cell: (i) => fixed(ps.mz[i], 4) }
      case 'rt':
        // negative or unknown (NaN): "–"
        return { cell: (i) => (ps.rt[i] >= 0 ? fixed(ps.rt[i] / 60, 2) : '–'), placeholder: (i) => !(ps.rt[i] >= 0) }
      case 'precursorError':
        return { cell: (i) => signed(ps.precursorErrorPpm[i], 2) }
      case 'score':
        return { cell: (i) => formatScore(ps.score[i], ps.qValue[i]) }
      case 'qValue':
        return { cell: (i) => formatQValue(ps.qValue[i]) }
      case 'pep':
        return { cell: (i) => formatQValue(ps.pep[i]) }
      case 'deltaMass':
        return { cell: (i) => signed(ps.deltaMass[i], 4), secondary: (i) => massShift(ps.deltaMass[i]) === null && ps.deltaMass[i] === ps.deltaMass[i], placeholder: (i) => ps.deltaMass[i] !== ps.deltaMass[i] }
      case 'file':
        return { cell: (i) => baseName(m.tables.files[ps.file[i]]) }
      case 'isotopeError':
        return { cell: (i) => String(ps.isotopeError[i]) }
      default:
        return { cell: () => '' }
    }
  }

  /** `compact`: a narrow list of these columns. */
  const compactColumns: Record<string, { width: number | 'auto'; header?: string; title?: string }> = {
    peptide: { width: 'auto' },
    charge: { width: 32 },
    sample: { width: 88 },
    qValue: { width: 64, header: 'q' },
    precursorError: { width: 58, header: 'Δ ppm', title: 'Precursor mass error (ppm)' },
    curation: { width: 28, header: '✓', title: 'Curation: accepted (✓) or rejected (✗)' },
  }

  const columns = $derived.by(() => {
    if (!compact) return tableColumns(level.visible, cells)
    // (one sample: its column would say the same on every row, and the peptides get the room)
    const specs = Object.entries(compactColumns).flatMap(([id, narrow]) => {
      if (id === 'sample' && m.samples.length <= 1) return []
      const spec = level.specs.find((s) => s.id === id)
      return spec ? [{ ...spec, ...narrow }] : []
    })
    return tableColumns(specs, (id) =>
      id === 'precursorError'
        ? { cell: (i: number) => signed(ps.precursorErrorPpm[i], 1) }
        : id === 'qValue'
          ? { cell: (i: number) => formatQCompact(ps.qValue[i]), cellTitle: (i: number) => String(ps.qValue[i]) }
          : cells(id),
    )
  })

  let table = $state<DataTable | null>(null)
  /** Makes the row the grid's active one, scrolls it into view and gives the grid the focus. */
  export function focusRow(row: number | null): void {
    table?.focusRow(row)
  }
</script>

{#snippet peptideCell(i: number)}
  {@const shift = massShift(ps.deltaMass[i])}
  {#if shift !== null}<span class="font-mono text-[12.5px] text-muted-foreground" title="Unassigned mass shift in Da. Its position on the peptide is not known."
      >[{shift}]?</span
    >{/if}<PeptideSequence view={peptideView(m.tables.peptides, ps.peptide[i], fixedMods)} />{#if ps.decoy[i]}<Marker
      text="decoy" />{/if}{#if isCoisolated(ps, i)}<Marker text="co-isolated" title={COISOLATED_TITLE} />{/if}
{/snippet}

{#snippet curationCell(i: number)}
  {@const mark = markOf(i)}
  {#if mark === ACCEPTED || mark === REJECTED}
    <span title={m.curationNotes.get(i)} class={mark === ACCEPTED ? 'text-emerald-700 dark:text-emerald-400' : 'text-destructive'}
      ><span aria-hidden="true">{mark === ACCEPTED ? '✓' : '✗'}</span><span class={compact ? 'sr-only' : 'ml-1'}>{MARK_NAMES[mark]}</span></span>
  {/if}
{/snippet}

{#snippet proteinsCell(i: number)}
  <ProteinRefs model={m} peptide={ps.peptide[i]} onOpen={onOpenProtein} />
{/snippet}

<DataTable
  bind:this={table}
  label="PSMs"
  rows={level.rows}
  {columns}
  sort={level.effectiveSort}
  onSort={(key) => level.sortBy(key)}
  onRowClick={onOpen}
  selected={level.selected}
  autoMinWidth={compact ? 80 : 220}
  followFocus={follow}
  {onBack}
  empty={level.emptyText} />
