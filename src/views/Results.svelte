<script lang="ts">
  // The results of the open search. One frame for every tab: the tabs (Overview, Proteins, Peptides, PSMs, Samples,
  // QC) and, on the three levels, the filter row stay where they are whatever is open; nothing is put above them or
  // takes their place. On a level a row opens its detail next to the table and the table stays: a protein its panel
  // and a peptide its detail beside the table, a PSM the PSM view under it (the spectrum wants the width). The
  // arrow keys in the table then go through the rows, the detail following, as do "Prev" and "Next". Enter on the
  // open row (or a double click) goes on to the level below for that row only: the Peptides and PSM tables narrowed
  // to the protein, the PSM table to the peptide, which the chips of the filter row say and take away; Backspace
  // goes back up, to the peptide of the PSM or the protein of the peptide. The handles between table and detail
  // size them (kept: format/settings). The Overview (the key figures, the files, and the mass shifts of an open
  // search), Samples and QC tabs show the search as a whole; Proteins stays the first view.
  import FaqLink from '$lib/components/help/FaqLink.svelte'
  import SaveNotice from '$lib/components/history/SaveNotice.svelte'
  import ColumnsMenu from '$lib/components/results/ColumnsMenu.svelte'
  import FilterBar from '$lib/components/results/FilterBar.svelte'
  import OverviewTab from '$lib/components/results/OverviewTab.svelte'
  import PeptidePanel from '$lib/components/peptide/PeptidePanel.svelte'
  import ProteinPanel from '$lib/components/protein/ProteinPanel.svelte'
  import PsmView from '$lib/components/psm/PsmView.svelte'
  import PeptidesTable from '$lib/components/results/PeptidesTable.svelte'
  import ProteinsEmpty from '$lib/components/results/ProteinsEmpty.svelte'
  import ProteinsTable from '$lib/components/results/ProteinsTable.svelte'
  import PsmsTable from '$lib/components/results/PsmsTable.svelte'
  import QcTab from '$lib/components/results/QcTab.svelte'
  import SamplesTab from '$lib/components/results/SamplesTab.svelte'
  import Splitter from '$lib/components/results/Splitter.svelte'
  import { Button } from '$lib/components/ui/button'
  import * as Tabs from '$lib/components/ui/tabs'
  import { displaySettings } from '$lib/format/settings.svelte'
  import { formatCount, parseAccession, parseDescription } from '$lib/results/display'
  import { exportCsv } from '$lib/results/exports'
  import { keyFigures, levelTotal } from '$lib/results/figures'
  import { inputNoticeItems, psmFdrAppliedToNone, psmFdrNotice } from '$lib/results/overview'
  import { restoreFromHistory } from '$lib/history/history.svelte'
  import { resultsViewFor, type LevelState, type Narrowing, type Tab } from '$lib/results/view.svelte'
  import { openResult } from '$lib/search/session.svelte'
  import { tick } from 'svelte'

  let { id }: { id: string | null } = $props()

  const result = $derived(openResult.current !== null && openResult.current.id === id ? openResult.current : null)
  const view = $derived(result ? resultsViewFor(result) : null)

  // An address of a search that is not open (after a restart): History may have it
  /** Why the last CSV export failed (the button's own export; the Export menu shows its errors itself). */
  let csvError = $state<string | null>(null)
  let restoring = $state<string | null>(null)
  let restoreFailed = $state<{ id: string; reason: string | null } | null>(null)
  $effect(() => {
    const wanted = id
    if (wanted === null || result !== null || restoring === wanted || restoreFailed?.id === wanted) return
    restoring = wanted
    restoreFromHistory(wanted)
      .then((found) => {
        if (!found) restoreFailed = { id: wanted, reason: null }
      })
      .catch((e) => (restoreFailed = { id: wanted, reason: String((e as Error)?.message ?? e) }))
      .finally(() => {
        if (restoring === wanted) restoring = null
      })
  })

  // at the search's cutoffs, without decoys: the key figures and the tabs' counts
  const figures = $derived(view ? keyFigures(view.model, view.result.outcome.summary) : null)
  const peptideTotal = $derived(view ? levelTotal(view.model, 'peptides', false) : 0)
  // the PSM FDR was requested but not applied to some file (no decoy PSM): its PSMs are not FDR-controlled
  const fdrNotice = $derived(view ? psmFdrNotice(view.result.outcome.summary) : null)
  // what ProSE reported about the inputs' spectra (no MS2 spectra, profile spectra, charges outside the range)
  const spectraNotices = $derived(view ? inputNoticeItems(view.result.outcome.summary) : [])
  const proteinTotal = $derived(view ? levelTotal(view.model, 'proteins', false) : 0)

  /** The rows a narrowed table shows (the tab says "4 of 132"); null: the table is not narrowed. */
  const narrowedRows = (level: LevelState) =>
    level.filters.proteinFilter !== undefined || level.filters.peptideFilter !== undefined ? level.rows.length : null
  const tabs = $derived<{ value: Tab; label: string; count?: number; narrowed?: number | null }[]>([
    { value: 'overview', label: 'Overview' },
    { value: 'proteins', label: 'Proteins', count: proteinTotal },
    { value: 'peptides', label: 'Peptides', count: peptideTotal, narrowed: view ? narrowedRows(view.peptides) : null },
    { value: 'psms', label: 'PSMs', count: figures?.psms ?? 0, narrowed: view ? narrowedRows(view.psms) : null },
    { value: 'samples', label: 'Samples' },
    { value: 'qc', label: 'QC' },
  ])

  // no protein passed the search's protein FDR, but there are identifications: say why
  const noProteins = $derived(
    view !== null && figures !== null && view.result.outcome.summary.proteinFdrApplied && figures.psms > 0 &&
      levelTotal(view.model, 'proteins', true) === 0,
  )

  const fixedMods = $derived(view?.result.settings.fixedMods ?? [])
  const fixedText = $derived(fixedMods.length > 0 ? fixedMods.join(', ') : 'none')

  // What is open next to the table of the tab that shows: the protein's panel, the peptide's detail, the PSM view
  const panelOpen = $derived(view !== null && view.tab === 'proteins' && view.panel !== null && !noProteins)
  const peptideOpen = $derived(view !== null && view.tab === 'peptides' && view.peptidePanel !== null)
  const psmOpen = $derived(view !== null && view.tab === 'psms' && view.psm !== null)
  const besideOpen = $derived(panelOpen || peptideOpen)
  /**
   * What the detail beside the table shows, for screen readers: the focus stays in the table while its arrow keys
   * go through the rows, and the panes are made anew for each row, so the region that says it is here, where it
   * stays and only its text changes. (The PSM view, which is not made anew, says its own.)
   */
  const announcement = $derived.by(() => {
    if (!view) return ''
    if (panelOpen && view.panel !== null) {
      const pr = view.model.tables.proteins
      return `Protein ${parseAccession(pr.accession[view.panel]).id}, ${parseDescription(pr.description[view.panel]).name}`
    }
    if (peptideOpen && view.peptidePanel !== null) return `Peptide ${view.model.tables.peptides.proforma[view.peptidePanel]}`
    return ''
  })

  let proteinsTable = $state<ProteinsTable | null>(null)
  let peptidesTable = $state<PeptidesTable | null>(null)
  let psmsTable = $state<PsmsTable | null>(null)

  // the sizes of the panes: the detail beside the table (its width), the PSM table above the PSM view (its height)
  let splitWidth = $state(1200)
  let splitHeight = $state(700)
  /** The least the table keeps beside a detail pane, and the least the pane takes. */
  const MIN_TABLE = 340
  const MIN_DETAIL = 380
  const detailWidth = $derived.by(() => {
    const wanted = displaySettings.detailWidth ?? Math.max(520, Math.min(640, splitWidth * 0.45))
    return Math.round(Math.max(MIN_DETAIL, Math.min(wanted, splitWidth - MIN_TABLE)))
  })
  /**
   * The PSM table above the PSM view: its header and four rows unless dragged (the open PSM and its neighbours: the
   * view wants the height; three rows in a low window); at least two rows, and the view keeps at least 280 px.
   */
  const ROW = 33
  const MIN_PSM_TABLE = 3 * ROW + 2
  const MIN_PSM_VIEW = 280
  const psmTableHeight = $derived.by(() => {
    // (a window lower than the default: one row fewer, so that the view keeps the fragment mass errors)
    const wanted = displaySettings.psmTableHeight ?? (splitHeight < 700 ? 4 : 5) * ROW + 2
    return Math.round(Math.max(MIN_PSM_TABLE, Math.min(wanted, splitHeight - MIN_PSM_VIEW)))
  })

  /** A row of the Proteins table: its panel, the focus staying in the table (the arrow keys go on). */
  function openProtein(protein: number) {
    view?.openProtein(protein)
  }

  /** A protein accession in the Peptides or PSM table, or "up" from a peptide or a PSM: the Proteins tab at the protein. */
  async function showProtein(protein: number) {
    if (!view) return
    view.showProtein(protein)
    await tick()
    proteinsTable?.focusRow(protein)
  }

  /** "Up" from a PSM (or Backspace in the PSM table): the Peptides tab at the peptide. */
  async function showPeptide(peptide: number) {
    if (!view) return
    view.showPeptide(peptide)
    await tick()
    peptidesTable?.focusRow(peptide)
  }

  /** The level below for one row: the Peptides or PSM table narrowed to a protein, the PSM table to a peptide. */
  async function narrowTo(what: Narrowing, tab: 'peptides' | 'psms' = 'psms') {
    if (!view) return
    view.narrowTo(what, tab)
    await tick()
    if (view.tab === 'peptides') peptidesTable?.focusRow(null)
    else psmsTable?.focusRow(null)
  }

  /** A PSM of the PSM table: the PSM view under the table, the focus staying in the table. */
  async function openPsm(psm: number) {
    if (!view) return
    view.openPsm(psm)
    // (the table got shorter above the view: the PSM's row stays in sight)
    await tick()
    psmsTable?.focusRow(psm)
  }

  /** A PSM of a protein's panel or a peptide's detail: the PSMs tab narrowed to them, the PSM open. */
  async function openPsmIn(psm: number, scope: { protein?: number | null; peptide?: number | null }) {
    if (!view) return
    view.openPsmIn(psm, scope)
    await tick()
    psmsTable?.focusRow(psm)
  }

  /** The focus to the table that shows, at its selected row (a chip of the filter row went: the arrow keys go on from there). */
  async function focusTable() {
    if (!view) return
    await tick()
    if (view.tab === 'peptides') peptidesTable?.focusRow(view.peptides.selected)
    else if (view.tab === 'psms') psmsTable?.focusRow(view.psms.selected)
  }

  /** Closes what is open next to the table that shows; the focus goes to its row. */
  async function closeDetail() {
    if (!view) return
    if (view.tab === 'proteins') {
      const protein = view.panel
      view.closeProtein()
      await tick()
      proteinsTable?.focusRow(protein)
    } else if (view.tab === 'peptides') {
      const peptide = view.peptidePanel
      view.closePeptide()
      await tick()
      peptidesTable?.focusRow(peptide)
    } else if (view.tab === 'psms' && view.psm) {
      const psm = view.psm.row
      view.closePsm()
      await tick()
      psmsTable?.focusRow(psm)
    }
  }

  /** Backspace in the PSM table: up to the peptide of its active row (else of the open or selected PSM); without one, the Peptides tab. */
  function upFromPsms(row: number | null = null) {
    if (!view) return
    const psm = row ?? view.psm?.row ?? view.psms.selected
    if (psm === null) view.tab = 'peptides'
    else void showPeptide(view.model.tables.psms.peptide[psm])
  }

  /** Backspace in the Peptides table: up to the protein of its active row (else of the open or selected peptide); without one, the Proteins tab. */
  function upFromPeptides(row: number | null = null) {
    if (!view) return
    const peptide = row ?? view.peptidePanel ?? view.peptides.selected
    const protein = peptide === null ? null : view.proteinOf(peptide)
    if (protein === null) view.tab = 'proteins'
    else void showProtein(protein)
  }

  // Escape closes what is open beside the table, from the pane and from the table (menus and fields that use
  // the key themselves have handled it first; the PSM view has its own Escape, which first leaves its tools)
  function onkeydown(event: KeyboardEvent) {
    if (event.key !== 'Escape' || event.defaultPrevented || !besideOpen || !(event.target instanceof Element)) return
    if (event.target.closest('input, textarea, [role="dialog"], [role="menu"]')) return
    // (from nowhere too: in a narrow window the table that had the focus gives way to the detail)
    const inside =
      event.target === document.body ||
      event.target.closest('[data-testid="protein-panel"], [data-testid="peptide-panel"], [role="grid"][aria-label="Proteins"], [role="grid"][aria-label="Peptides"]')
    if (inside) {
      event.preventDefault()
      void closeDetail()
    }
  }
</script>

<svelte:window {onkeydown} />

<!-- What the result says about itself, above the tabs on every tab: the History save, PSMs without an FDR, ProSE's
     notes on the inputs' spectra, ProSE's warnings -->
{#snippet notices()}
  {#if view}
    <SaveNotice id={view.result.id} />
    {#if fdrNotice}
      <p class="text-xs text-amber-700" data-testid="psm-fdr-notice">{fdrNotice}<FaqLink faq="faq-decoys-fdr" /></p>
    {/if}
    {#if spectraNotices.length > 0}
      <ul class="flex flex-col gap-0.5 text-xs text-amber-700 dark:text-amber-400" data-testid="input-notices">
        {#each spectraNotices as notice (notice.text)}
          <li>{notice.text}<FaqLink check={notice.kind} /></li>
        {/each}
      </ul>
    {/if}
    {#if view.result.outcome.warnings.length > 0}
      <p class="text-xs text-amber-700 dark:text-amber-400" data-testid="results-warnings">{view.result.outcome.warnings.join(' ')}</p>
    {/if}
  {/if}
{/snippet}

{#if view && figures}
  {@const level = view.level}
  <section data-testid="view-results" class="absolute inset-0 flex flex-col gap-3 px-5 pt-4 pb-3">
    <h1 class="sr-only">Results</h1>
    <p class="sr-only" aria-live="polite" data-testid="detail-announcement">{announcement}</p>
    {@render notices()}

    <Tabs.Root bind:value={view.tab} class="flex min-h-0 flex-1 flex-col gap-3">
      <!-- the tabs: always here, whatever is open (a table narrowed to a protein or a peptide says how many it shows) -->
      <div class="flex min-h-[41px] flex-wrap items-center justify-between gap-2 border-b">
        <!-- scrolls sideways where the window is too narrow for all tabs -->
        <Tabs.List class="max-w-full overflow-x-auto" data-testid="results-tabs">
          {#each tabs as t (t.value)}
            <Tabs.Trigger value={t.value} class="px-3 text-[13px]">
              {t.label}{#if t.narrowed != null}<span class="ml-1 text-xs font-medium text-primary tabular-nums" data-testid="tab-narrowed" title="Narrowed: {formatCount(t.narrowed)} of {formatCount(t.count ?? 0)}. The chips of its filter row say to what."
                  >{formatCount(t.narrowed)} of {formatCount(t.count ?? 0)}</span
                >{:else if t.count !== undefined}<span class="ml-1 text-xs font-normal text-muted-foreground tabular-nums"
                  >{formatCount(t.count)}</span
                >{/if}
            </Tabs.Trigger>
          {/each}
        </Tabs.List>
        {#if level}
          <div class="flex items-center gap-2">
            <ColumnsMenu {view} {level} />
            <Button size="sm" class="h-7 px-2.5 text-xs" disabled={view.exporting} onclick={() => exportCsv(view).then(() => (csvError = null), (e) => (csvError = String((e as Error)?.message ?? e)))}>
              {#if view.exporting}
                Exporting…
              {:else}
                Export CSV · {formatCount(level.rows.length)}
                {level.rows.length === 1 ? 'row' : 'rows'}
              {/if}
            </Button>
            {#if csvError}<span class="text-xs text-destructive" role="alert" data-testid="csv-error">Export failed: {csvError}</span>{/if}
          </div>
        {/if}
      </div>

      {#if level}
        <Tabs.Content value={view.tab} tabindex={-1} class="flex min-h-0 flex-1 flex-col gap-3" data-testid="results-table">
          <!-- the filter row: over the table and its detail, so that it keeps its width (and its place) when a detail opens -->
          {#key level}
            <FilterBar
              {view}
              {level}
              proteinFdrApplied={view.result.outcome.summary.proteinFdrApplied}
              psmFdrApplied={!psmFdrAppliedToNone(view.result.outcome.summary)}
              onWiden={focusTable} />
          {/key}

          <div
            class="@container/split flex min-h-0 flex-1 {level.level === 'psms' ? 'flex-col' : ''}"
            bind:clientWidth={splitWidth}
            bind:clientHeight={splitHeight}
            data-testid="results-split">
            <!-- the table: beside a detail in a narrow window it gives way to the detail (the tabs and the filter row stay) -->
            <div
              class="flex min-h-0 min-w-0 flex-col gap-2 {psmOpen ? 'flex-none' : 'flex-1'} {besideOpen ? '@max-[760px]/split:hidden' : ''}"
              style:height={psmOpen ? `${psmTableHeight}px` : undefined}
              data-level-table
              data-testid="results-table-area">
              {#if level.level === 'proteins' && noProteins}
                <ProteinsEmpty {view} peptides={peptideTotal} psms={figures.psms} />
              {:else if level.level === 'proteins'}
                <ProteinsTable bind:this={proteinsTable} {view} {level} onOpen={openProtein} onDrill={(protein) => narrowTo({ protein }, 'peptides')} />
              {:else if level.level === 'peptides'}
                <PeptidesTable
                  bind:this={peptidesTable}
                  {view}
                  {level}
                  onOpen={(peptide) => view.openPeptide(peptide)}
                  onDrill={(peptide) => narrowTo({ peptide })}
                  onBack={upFromPeptides}
                  onOpenProtein={showProtein} />
              {:else}
                <PsmsTable bind:this={psmsTable} {view} {level} onOpen={openPsm} follow={psmOpen} onBack={upFromPsms} onOpenProtein={showProtein} />
              {/if}
              {#if level.level !== 'proteins' && !psmOpen}
                <p class="text-xs text-muted-foreground" data-testid="mod-legend">
                  {#if displaySettings.modMode === 'marked'}
                    <span class="legend-mod">M<sup>ox</sup></span> modified residue, hover to see the modification · fixed
                    modifications ({fixedText}) are not marked
                  {:else}
                    ProForma with fixed modifications ({fixedText})
                  {/if}
                  · CSV and copy use ProForma
                </p>
              {/if}
            </div>

            {#if panelOpen && view.panel !== null}
              <div class="@max-[760px]/split:hidden flex flex-none">
                <Splitter
                  axis="x"
                  invert
                  label="Width of the protein's panel"
                  value={detailWidth}
                  min={MIN_DETAIL}
                  max={splitWidth - MIN_TABLE}
                  onChange={(w) => (displaySettings.detailWidth = w)}
                  onReset={() => (displaySettings.detailWidth = null)} />
              </div>
              <div class="relative flex min-h-0 min-w-0 flex-none overflow-y-auto @max-[760px]/split:!w-full @max-[760px]/split:flex-1" style:width="{detailWidth}px" data-testid="results-detail">
                {#key view.panel}
                  <ProteinPanel
                    {view}
                    protein={view.panel}
                    onClose={closeDetail}
                    onNarrow={narrowTo}
                    onOpenPsm={(psm, peptide) => openPsmIn(psm, { protein: view.panel, peptide })} />
                {/key}
              </div>
            {:else if peptideOpen && view.peptidePanel !== null}
              <div class="@max-[760px]/split:hidden flex flex-none">
                <Splitter
                  axis="x"
                  invert
                  label="Width of the peptide's detail"
                  value={detailWidth}
                  min={MIN_DETAIL}
                  max={splitWidth - MIN_TABLE}
                  onChange={(w) => (displaySettings.detailWidth = w)}
                  onReset={() => (displaySettings.detailWidth = null)} />
              </div>
              <div class="relative flex min-h-0 min-w-0 flex-none overflow-y-auto @max-[760px]/split:!w-full @max-[760px]/split:flex-1" style:width="{detailWidth}px" data-testid="results-detail">
                {#key view.peptidePanel}
                  <PeptidePanel
                    {view}
                    peptide={view.peptidePanel}
                    onClose={closeDetail}
                    onNarrow={narrowTo}
                    onOpenProtein={showProtein}
                    onOpenPsm={(psm) => openPsmIn(psm, { peptide: view.peptidePanel })} />
                {/key}
              </div>
            {:else if psmOpen}
              <Splitter
                axis="y"
                label="Height of the PSM table"
                value={psmTableHeight}
                min={MIN_PSM_TABLE}
                max={splitHeight - MIN_PSM_VIEW}
                onChange={(h) => (displaySettings.psmTableHeight = h)}
                onReset={() => (displaySettings.psmTableHeight = null)} />
              <!-- (it fits its height; only a window lower than the view's least height makes it scroll) -->
              <div class="relative flex min-h-0 min-w-0 flex-1 overflow-y-auto" data-testid="results-detail">
                <PsmView {view} onClose={closeDetail} onUp={(to) => ('peptide' in to ? showPeptide(to.peptide) : showProtein(to.protein))} />
              </div>
            {/if}
          </div>
        </Tabs.Content>
      {:else}
        <!-- Overview, Samples and QC: the search as a whole; they scroll in their own box -->
        <Tabs.Content value={view.tab} tabindex={-1} class="min-h-0 flex-1 overflow-y-auto" data-testid="{view.tab}-tab">
          {#if view.tab === 'overview'}
            <OverviewTab {view} />
          {:else if view.tab === 'samples'}
            <SamplesTab {view} />
          {:else}
            <QcTab {view} />
          {/if}
        </Tabs.Content>
      {/if}
    </Tabs.Root>
  </section>
{:else}
  <section data-testid="view-results" class="mx-auto max-w-[1120px] px-5 pt-6 pb-24">
    <h1 class="mb-1 text-xl font-semibold tracking-tight">Results</h1>
    <p class="mb-5 text-muted-foreground" data-testid="results-none">
      {#if id === null}No result is open. Start a search, or open one from History.{:else if restoring === id}Opening result {id}
        from History…{:else if restoreFailed?.id === id && restoreFailed.reason}Result {id} could not be opened from
        History: {restoreFailed.reason}{:else}Result {id} is not in History. Open a search from History instead.{/if}
    </p>
  </section>
{/if}

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
