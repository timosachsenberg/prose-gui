<script lang="ts">
  // The panel of one protein, beside the Proteins table: its accession and name, the share of
  // its sequence that peptides cover, the strip of those peptides with the modified sites, and the
  // tabs Coverage (the sequence as a map), Peptides (expanding to their PSMs) and PSMs. A labelled
  // region, not a dialog: the table beside it stays usable, and keeps the focus, so that its arrow keys go
  // through the proteins with the panel following ("Prev" and "Next" do the same). Escape closes it (Results
  // handles the key, so that it works from the table as well). "Peptides ›" and "PSMs ›" go on to the level
  // below, narrowed to this protein; a PSM of the panel opens in the PSMs tab, among the PSMs of the protein (and
  // of its peptide).
  import Marker from '$lib/components/results/Marker.svelte'
  import { Button } from '$lib/components/ui/button'
  import * as Tabs from '$lib/components/ui/tabs'
  import { displaySettings } from '$lib/format/settings.svelte'
  import { formatCount, parseAccession, parseDescription } from '$lib/results/display'
  import { openHelp } from '$lib/help-link.svelte'
  import { groupMates, proteinDetail } from '$lib/results/protein'
  import type { Narrowing, ResultsView } from '$lib/results/view.svelte'
  import StepButtons from '../results/StepButtons.svelte'
  import XIcon from '@lucide/svelte/icons/x'
  import CoverageMap from './CoverageMap.svelte'
  import CoverageStrip from './CoverageStrip.svelte'
  import ProteinPeptides from './ProteinPeptides.svelte'
  import ProteinPsms from './ProteinPsms.svelte'

  let {
    view,
    protein,
    onClose,
    onNarrow,
    onOpenPsm,
  }: {
    view: ResultsView
    /** The protein: a row of `tables.proteins`. */
    protein: number
    onClose: () => void
    /** The level below: the Peptides or the PSMs tab narrowed to this protein, the PSMs tab to one of its peptides. */
    onNarrow: (what: Narrowing, tab?: 'peptides' | 'psms') => void
    /** A PSM of the panel: of a peptide of the Peptides tab (`peptide`), or of the PSMs tab (null). */
    onOpenPsm: (psm: number, peptide: number | null) => void
  } = $props()

  const m = $derived(view.model)
  const pr = $derived(m.tables.proteins)
  const fixedMods = $derived(view.result.settings.fixedMods)
  const detail = $derived(proteinDetail(m, protein, fixedMods))
  const accession = $derived(parseAccession(pr.accession[protein]))
  const description = $derived(parseDescription(pr.description[protein]))
  const decoy = $derived(pr.decoy[protein] === 1)
  const mates = $derived(groupMates(m, protein))
  const mateNames = $derived(mates.map((q) => parseAccession(pr.accession[q]).id))
  const fixedText = $derived(fixedMods.length > 0 ? fixedMods.join(', ') : 'none')

  // the peptide that is open in the Peptides tab (a row of `tables.peptides`): the strip shows it. The
  // view keeps it, so that it is still open when the user comes back from a PSM
  const selectedSpans = $derived(detail.peptides.find((e) => e.peptide === view.panelPeptide)?.spans ?? [])

  const badge = 'inline-flex h-5 items-center rounded-[5px] border bg-muted px-[7px] text-[11.5px] font-medium whitespace-nowrap'
</script>

<section
  aria-label="Protein {accession.id}"
  data-testid="protein-panel"
  class="flex h-full min-h-[26rem] min-w-0 flex-1 flex-col bg-background pl-3 @max-[760px]/split:pl-0">
  <header class="flex items-start justify-between gap-2">
    <div class="min-w-0">
      <div class="truncate font-mono text-xs text-muted-foreground" title={pr.accession[protein]}>
        <span data-testid="panel-accession">{accession.id}</span>{#if accession.entry}{' · '}{accession.entry}{/if}{#if detail.length > 0}{' · '}{formatCount(detail.length)} aa{/if}{#if accession.contaminant}<Marker text="contaminant" />{/if}{#if decoy}<span data-testid="panel-decoy"><Marker text="decoy" /></span>{/if}
      </div>
      <h2 class="mt-0.5 text-xl font-semibold tracking-tight">
        {description.name || accession.id}
      </h2>
      <!-- the focus stays in the table: say which protein the panel shows -->
      <p class="sr-only" aria-live="polite">Protein {accession.id}, {description.name}</p>
      {#if mates.length > 0}
        <p class="mt-0.5 text-xs text-muted-foreground" data-testid="panel-group" title={mates.map((q) => pr.accession[q]).join('\n')}>
          Indistinguishable from {mateNames.slice(0, 4).join(', ')}{#if mates.length > 4}
            and {mates.length - 4} more{/if}
          <button type="button" class="ml-1 underline underline-offset-2" onclick={() => openHelp('help-groups')}>Protein groups ›</button>
        </p>
      {/if}
    </div>
    <div class="flex flex-none items-center gap-1.5">
      <StepButtons neighbours={view.neighbours('proteins')} noun="protein" onStep={(direction) => view.step('proteins', direction)} />
      <Button variant="ghost" size="icon-sm" aria-label="Close protein panel" title="Close (Esc)" onclick={onClose}>
        <XIcon aria-hidden="true" />
      </Button>
    </div>
  </header>

  <div class="mt-2.5 flex flex-wrap gap-1.5">
    <span class={badge} data-testid="panel-coverage" title="Residues covered by peptides with PSMs at the search's PSM FDR"
      >Coverage {(detail.coverage * 100).toFixed(1)} %</span>
    <span class={badge} data-testid="panel-peptides" title="Peptides counted by plain sequence. Modified forms count once."
      >{formatCount(detail.sequences)} {detail.sequences === 1 ? 'peptide' : 'peptides'}</span>
    <span class={badge} data-testid="panel-psms">{formatCount(detail.psms)} PSMs</span>
    <span
      class="{badge} {detail.sites.length > 0 ? 'border-mod-border bg-mod-bg text-mod-fg' : 'text-muted-foreground'}"
      data-testid="panel-sites"
      title="Residues with a modification in peptides with PSMs. Fixed modifications are not counted."
      >{detail.sites.length} modified {detail.sites.length === 1 ? 'site' : 'sites'}</span>
  </div>

  <CoverageStrip {detail} selected={selectedSpans} />

  <!-- on to the level below, for this protein only (Enter on its row, or a double click, does the first) -->
  <div class="mt-2 flex flex-wrap items-center gap-1.5" data-testid="panel-narrow">
    <Button size="sm" class="h-7 px-2.5 text-xs" title="The Peptides tab with the peptides of this protein only (Enter in the table)" onclick={() => onNarrow({ protein }, 'peptides')}
      >Peptides of {accession.id} ›</Button>
    <Button variant="outline" size="sm" class="h-7 px-2.5 text-xs" title="The PSMs tab with the PSMs of this protein only" onclick={() => onNarrow({ protein }, 'psms')}
      >PSMs of {accession.id} ›</Button>
  </div>

  <Tabs.Root bind:value={view.panelTab} class="mt-2.5 flex min-h-0 flex-1 flex-col gap-2.5">
    <div class="flex flex-wrap items-center justify-between gap-2">
      <Tabs.List class="min-w-0 flex-1 justify-start border-b">
        <Tabs.Trigger value="coverage" class="flex-none px-3 text-[13px]"
          >Coverage{#if detail.length > 0}<span class="ml-1 text-xs font-normal text-muted-foreground tabular-nums">{(detail.coverage * 100).toFixed(0)} %</span>{/if}</Tabs.Trigger
        >
        <Tabs.Trigger value="peptides" class="flex-none px-3 text-[13px]"
          >Peptides<span class="ml-1 text-xs font-normal text-muted-foreground tabular-nums">{formatCount(detail.peptides.length)}</span></Tabs.Trigger>
        <Tabs.Trigger value="psms" class="flex-none px-3 text-[13px]"
          >PSMs<span class="ml-1 text-xs font-normal text-muted-foreground tabular-nums">{formatCount(detail.psms)}</span></Tabs.Trigger>
      </Tabs.List>
    </div>

    <!-- only the open tab is rendered: the map of a long protein is thousands of nodes, the PSM list a sort -->
    <Tabs.Content value="coverage" class="min-h-0 flex-1 overflow-auto pt-1">
      {#if view.panelTab === 'coverage'}
        {#if detail.length === 0}
          <p class="py-8 text-center text-muted-foreground">This protein has no sequence in the result, so there is no coverage map.</p>
        {:else}
          {@const coveredResidues = detail.covered.reduce((n, c) => n + c, 0)}
          <p class="pb-2 text-xs text-muted-foreground tabular-nums" data-testid="coverage-summary">
            <span class="text-[15px] font-semibold text-foreground">{(detail.coverage * 100).toFixed(1)} %</span> sequence coverage ·
            {formatCount(coveredResidues)} of {formatCount(detail.length)} residues · {formatCount(detail.sequences)}
            {detail.sequences === 1 ? 'peptide' : 'peptides'} with PSMs at the PSM FDR
          </p>
          <CoverageMap {detail} sequence={pr.sequence[protein]} />
        {/if}
      {/if}
    </Tabs.Content>
    <Tabs.Content value="peptides" class="min-h-0 flex-1 overflow-auto">
      {#if view.panelTab === 'peptides'}
        <ProteinPeptides {view} {detail} bind:expanded={view.panelPeptide} onOpenPsm={(psm) => onOpenPsm(psm, view.panelPeptide)} onOpenPsmTable={(what) => onNarrow(what, 'psms')} />
      {/if}
    </Tabs.Content>
    <Tabs.Content value="psms" class="flex min-h-0 flex-1 flex-col">
      {#if view.panelTab === 'psms'}
        <ProteinPsms {view} {detail} label="PSMs of {accession.id}" onOpenPsm={(psm) => onOpenPsm(psm, null)} />
      {/if}
    </Tabs.Content>
  </Tabs.Root>

  <!-- what a dot is, the strip says; this says what a marked residue is and what is not marked -->
  <p class="mt-2 text-xs text-muted-foreground" data-testid="panel-legend">
    {#if displaySettings.modMode === 'marked'}
      <span class="legend-mod">M<sup>ox</sup></span> modified residue, hover to see the modification{#if fixedMods.length > 0}{' · '}fixed
        modifications ({fixedText}) are not marked{/if}
    {:else}
      ProForma shows all modifications{#if fixedMods.length > 0}, including the fixed ones ({fixedText}). Only the others appear as
        modified-site dots{/if}
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
