<script lang="ts">
  // The detail of one peptide (a modified form of a sequence is a peptide of its own), beside the Peptides table, as
  // the protein's panel stands beside the Proteins table: its sequence with the modifications, how many PSMs it has
  // and where it was found, its proteins (each opens in the Proteins tab) and its PSMs within the search's PSM FDR,
  // best q-value first. A PSM opens in the PSMs tab among the PSMs of the peptide; "PSMs of this peptide ›" (Enter
  // on its row, or a double click) goes on to that table. A labelled region, not a dialog: the table keeps the focus,
  // so that its arrow keys go through the peptides with the detail following ("Prev" and "Next" do the same); Escape
  // closes it (Results handles the key).
  import PeptideSequence from '$lib/components/PeptideSequence.svelte'
  import Marker from '$lib/components/results/Marker.svelte'
  import StepButtons from '$lib/components/results/StepButtons.svelte'
  import { Button } from '$lib/components/ui/button'
  import { peptideView } from '$lib/format/peptide'
  import { displaySettings } from '$lib/format/settings.svelte'
  import { ACCEPTED, MARK_NAMES, REJECTED, peptideMarks } from '$lib/results/curation'
  import { formatCount, formatQValue, parseAccession, parseDescription, spectrumLabel } from '$lib/results/display'
  import { defaultFilters, sortRows } from '$lib/results/filters'
  import { peptidePsms } from '$lib/results/protein'
  import { spectrumTitle } from '$lib/results/table-columns'
  import type { Narrowing, ResultsView } from '$lib/results/view.svelte'
  import XIcon from '@lucide/svelte/icons/x'
  import DataTable, { type Column, type Sort } from '../results/DataTable.svelte'
  import { signed } from '../results/cells'

  let {
    view,
    peptide,
    onClose,
    onNarrow,
    onOpenProtein,
    onOpenPsm,
  }: {
    view: ResultsView
    /** The peptide: a row of `tables.peptides`. */
    peptide: number
    onClose: () => void
    /** The level below: the PSMs tab narrowed to this peptide. */
    onNarrow: (what: Narrowing) => void
    /** A protein of the peptide: the Proteins tab at it. */
    onOpenProtein: (protein: number) => void
    /** A PSM of the peptide: the PSMs tab narrowed to the peptide, the PSM open. */
    onOpenPsm: (psm: number) => void
  } = $props()

  const m = $derived(view.model)
  const pt = $derived(m.tables.peptides)
  const ps = $derived(m.tables.psms)
  const pr = $derived(m.tables.proteins)
  const fixedMods = $derived(view.result.settings.fixedMods)
  const fixedText = $derived(fixedMods.length > 0 ? fixedMods.join(', ') : 'none')
  const shown = $derived(peptideView(pt, peptide, fixedMods))
  const proforma = $derived(pt.proforma[peptide])
  const decoy = $derived(pt.decoy[peptide] === 1)

  /** Its proteins with where the peptide is in each (1-based, inclusive; null: not known). */
  const proteins = $derived.by(() => {
    const out: { row: number; id: string; name: string; accession: string; where: string | null }[] = []
    for (let r = pt.proteinOffsets[peptide]; r < pt.proteinOffsets[peptide + 1]; r++) {
      const p = pt.protein[r]
      const accession = pr.accession[p]
      const [start, end] = [pt.start[r], pt.end[r]]
      out.push({
        row: p,
        id: parseAccession(accession).id,
        name: parseDescription(pr.description[p]).name,
        accession,
        where: end > start ? `${formatCount(start + 1)}–${formatCount(end)}` : null,
      })
    }
    return out
  })

  // its PSMs within the search's PSM FDR (those the tables count), sorted as the user asks (best q-value first)
  let sort = $state<Sort>({ key: 'qValue', dir: 'asc' })
  const all = $derived(peptidePsms(m, peptide))
  const rows = $derived(sortRows(m, 'psms', all, sort.key, sort.dir, defaultFilters()))
  const numeric = new Set(['charge', 'precursorErrorPpm', 'qValue'])
  function onSort(key: string) {
    sort = sort.key === key ? { key, dir: sort.dir === 'asc' ? 'desc' : 'asc' } : { key, dir: numeric.has(key) ? 'desc' : 'asc' }
  }

  /** The samples that found it (columns of the tables), with their PSM counts. */
  const found = $derived.by(() => {
    const nS = m.samples.length
    return m.samples.map((s, i) => ({ label: s.label, psms: m.peptide.perSample[peptide * nS + i] })).filter((s) => s.psms > 0)
  })
  /** Its PSMs' curation marks (the marks change in place: read with their count of changes). */
  const marks = $derived.by(() => {
    void view.curationVersion
    const all = peptideMarks(m)
    return { accepted: all.accepted[peptide], rejected: all.rejected[peptide] }
  })
  const markOf = (i: number) => {
    void view.curationVersion
    return m.curation[i]
  }

  const columns = $derived<Column[]>([
    { id: 'sample', header: 'Sample', width: 'auto', align: 'left', sortKey: 'sample', cell: (i) => m.samples[m.psmSample[i]].label },
    { id: 'scan', header: 'Scan', width: 72, align: 'left', sortKey: 'nativeId', title: spectrumTitle, cell: (i) => spectrumLabel(ps.nativeId[i]), cellTitle: (i) => ps.nativeId[i] },
    { id: 'charge', header: 'z', width: 38, align: 'right', sortKey: 'charge', title: 'Charge', cell: (i) => String(ps.charge[i]) },
    { id: 'error', header: 'Δ ppm', width: 76, align: 'right', sortKey: 'precursorErrorPpm', title: 'Precursor mass error', cell: (i) => signed(ps.precursorErrorPpm[i], 2) },
    { id: 'qValue', header: 'q-value', width: 96, align: 'right', sortKey: 'qValue', cell: (i) => formatQValue(ps.qValue[i]) },
    { id: 'curation', header: '', width: 30, align: 'left', title: 'Your mark: accepted or rejected', cell: () => markCell },
  ])

  const badge = 'inline-flex h-5 items-center rounded-[5px] border bg-muted px-[7px] text-[11.5px] font-medium whitespace-nowrap'
</script>

{#snippet markCell(i: number)}
  {@const mark = markOf(i)}
  {#if mark === ACCEPTED || mark === REJECTED}
    <span class={mark === ACCEPTED ? 'text-emerald-600' : 'text-destructive'} title={MARK_NAMES[mark]}
      ><span aria-hidden="true">{mark === ACCEPTED ? '✓' : '✗'}</span><span class="sr-only">{MARK_NAMES[mark]}</span></span>
  {/if}
{/snippet}

<section aria-label="Peptide {proforma}" data-testid="peptide-panel" class="flex h-full min-h-[25rem] min-w-0 flex-1 flex-col bg-background pl-3 @max-[760px]/split:pl-0">
  <header class="flex items-start justify-between gap-2">
    <div class="min-w-0">
      <div class="text-xs text-muted-foreground">
        Peptide · {formatCount(pt.sequence[peptide].length)} aa{#if decoy}<span data-testid="peptide-decoy"><Marker text="decoy" /></span>{/if}{#if m.peptide.contaminant[peptide]}<Marker
            text="contaminant" />{/if}
      </div>
      <h2 class="mt-0.5 text-xl leading-snug font-semibold [overflow-wrap:anywhere]" title={proforma} data-testid="peptide-sequence">
        <PeptideSequence view={shown} class="!text-[19px] !whitespace-normal" />
      </h2>
      <!-- the focus stays in the table: say which peptide the detail shows -->
      <p class="sr-only" aria-live="polite">Peptide {proforma}</p>
    </div>
    <div class="flex flex-none items-center gap-1.5">
      <StepButtons neighbours={view.neighbours('peptides')} noun="peptide" onStep={(direction) => view.step('peptides', direction)} />
      <Button variant="ghost" size="icon-sm" aria-label="Close the peptide's detail" title="Close (Esc)" onclick={onClose}>
        <XIcon aria-hidden="true" />
      </Button>
    </div>
  </header>

  <div class="mt-2.5 flex flex-wrap gap-1.5">
    <span class={badge} data-testid="peptide-psms" title="PSMs of this peptide within the search's PSM FDR">{formatCount(all.length)} {all.length === 1 ? 'PSM' : 'PSMs'}</span>
    <span class={badge} data-testid="peptide-found" title="Samples (condition and replicate) with a PSM of this peptide"
      >found in {m.peptide.foundIn[peptide]} of {m.design.samples.length} {m.design.samples.length === 1 ? 'sample' : 'samples'}</span>
    <span class={badge} title="The lowest q-value of its PSMs">best q {formatQValue(m.peptide.bestQ[peptide])}</span>
    {#if marks.accepted > 0}<span class="{badge} border-emerald-600/40 text-emerald-700 dark:text-emerald-400" data-testid="peptide-accepted">✓ {formatCount(marks.accepted)} accepted</span>{/if}
    {#if marks.rejected > 0}<span class="{badge} border-destructive/40 text-destructive" data-testid="peptide-rejected">✗ {formatCount(marks.rejected)} rejected</span>{/if}
  </div>

  <!-- up: the proteins the peptide is of (Backspace in the table goes to the first) -->
  <div class="mt-3" data-testid="peptide-proteins">
    <h3 class="mb-1 text-xs font-medium text-muted-foreground">{proteins.length === 1 ? 'Protein' : `Proteins · ${formatCount(proteins.length)}`}</h3>
    {#if proteins.length === 0}
      <p class="text-xs text-muted-foreground italic" title="None of this peptide's proteins pass the protein FDR">below protein FDR</p>
    {:else}
      <ul class="flex max-h-[5.5rem] flex-col gap-0.5 overflow-y-auto text-[13px]">
        {#each proteins as protein (protein.row)}
          <li class="flex min-w-0 items-baseline gap-2">
            <button
              type="button"
              class="flex-none rounded-sm font-mono text-[12.5px] text-primary underline-offset-2 hover:underline focus-visible:ring-2 focus-visible:ring-ring/50 focus-visible:outline-none"
              title="Open {protein.accession} in the Proteins tab"
              onclick={() => onOpenProtein(protein.row)}>{protein.id} ↑</button>
            <span class="min-w-0 truncate" title={protein.name}>{protein.name}</span>
            {#if protein.where}<span class="ml-auto flex-none text-xs text-muted-foreground tabular-nums" title="The peptide's residues in the protein">{protein.where}</span>{/if}
          </li>
        {/each}
      </ul>
    {/if}
  </div>

  {#if found.length > 0 && m.samples.length > 1}
    <div class="mt-3" data-testid="peptide-samples">
      <h3 class="mb-1 text-xs font-medium text-muted-foreground">PSMs per sample</h3>
      <p class="flex max-h-[3.4rem] flex-wrap gap-x-3 gap-y-0.5 overflow-y-auto text-[13px] tabular-nums">
        {#each found as s (s.label)}<span class="whitespace-nowrap">{s.label} <span class="text-muted-foreground">·</span> {formatCount(s.psms)}</span>{/each}
      </p>
    </div>
  {/if}

  <!-- down: its PSMs (a row opens the PSM in the PSMs tab), and on to the PSM table for this peptide only -->
  <div class="mt-3 mb-1.5 flex flex-wrap items-center justify-between gap-2">
    <h3 class="text-xs font-medium text-muted-foreground">Its PSMs · click one to open its spectrum</h3>
    <Button size="sm" class="h-7 px-2.5 text-xs" title="The PSMs tab with the PSMs of this peptide only (Enter in the table)" data-testid="peptide-narrow" onclick={() => onNarrow({ peptide })}
      >PSMs of this peptide ›</Button>
  </div>
  <div class="flex min-h-0 flex-1 flex-col">
    <DataTable label="PSMs of {proforma}" {rows} {columns} {sort} {onSort} onRowClick={onOpenPsm} autoMinWidth={110} empty="No PSM of this peptide passes the search's PSM FDR." />
  </div>

  <p class="mt-2 text-xs text-muted-foreground" data-testid="peptide-legend">
    {#if displaySettings.modMode === 'marked'}
      <span class="legend-mod">M<sup>ox</sup></span> modified residue, hover to see the modification{#if fixedMods.length > 0}{' · '}fixed
        modifications ({fixedText}) are not marked{/if}
    {:else}
      ProForma shows all modifications{#if fixedMods.length > 0}, including the fixed ones ({fixedText}){/if}
    {/if}
  </p>
</section>

<style>
  .legend-mod {
    font-family: var(--font-sequence);
    background: var(--mod-bg);
    color: var(--mod-fg);
    border-radius: 3px;
    padding: 0 1px;
    box-shadow: inset 0 -1.5px 0 var(--mod);
  }
  .legend-mod sup {
    font-size: 9px;
    font-weight: 600;
    line-height: 0;
    margin-left: 1px;
  }
</style>
