<script lang="ts">
  // The PSM view (mockup 3), under the PSM table, whose arrow keys go through the PSMs while the spectrum stays in
  // sight: the tabs and the table's filter row stay above both. It takes the height it is given and never scrolls
  // as a whole: the spectrum is as high as fits (SpectrumMirror's `fit`), the fragment mass errors under it give
  // way when the view is too low for both, and the PSM's features and the ion table stand beside the spectrum in a
  // column with two tabs ("Features · Fragments" shows and hides it; it shows by itself only in a view wide enough
  // for it and a spectrum of full width; only a long ion table or the list of all features scrolls, in that
  // column; in a narrow view it lies over the spectrum). Its header: the peptide with its fragment ladder,
  // "Accept"/"Reject", its place in the table ("3 of 132") with Prev/Next, and "Export spectrum (SVG)"; under it
  // what the PSM is, the ways up (the PSM's peptide in the Peptides tab, its protein in the Proteins tab), and the note;
  // the peptide with its fragment ladder; the spectrum annotated (psm/annotate.ts, the web module's
  // annotatePeaks in TypeScript), mirrored against PeptDeep's prediction, which ProSE wrote for every
  // PSM (its input's QPX psm file, read when the PSM is opened), or the theoretical spectrum, with ion,
  // charge, precursor and loss toggles (the precursor peaks shown unless switched off; the precursor's m/z is marked
  // in the spectrum always); the
  // ion table, the fragment mass errors of the annotated ions shown (a strip that can be collapsed, remembered
  // in displaySettings.massErrors) and the PSM's features. A PSM whose peaks the search did not
  // keep shows its ladder, theoretical ions and features. An open search's unlocalized mass shift is
  // shown, and the annotation is of the peptide without it. A PSM of a co-isolated precursor is marked, and the
  // other PSMs of the same spectrum are linked (results/coisolated.ts).
  // The toolbar's "Measure" and "Label" (one at a time) are the tools of pyopenms-viewer and openms-viewer on the
  // observed peaks (SpectrumMirror, psm/measure.ts): Δm/z between two peaks, with what it may be (a residue of the
  // search, unmodified or with one of its fixed or variable modifications), and texts of one's own; kept per spectrum
  // for the session, as the viewers keep them (view.psmMarks: the PSMs of one spectrum, its co-isolated precursors,
  // share them), cleared with "Clear Δ" and "Clear labels". M and L switch the tools (as openms-viewer's Ctrl+M and
  // Ctrl+L), unless what has the focus takes the keys. Esc first cancels what is half done (a measurement, a label being
  // written) or the selection, then leaves the tool, then closes the view; Delete removes what is selected.
  // "Accept" and "Reject" (keys A and R; again: the mark goes) curate the PSM (results/curation.ts, view.curate): the
  // PSM table shows and filters the marks, and a PSM that the table's curation filter now hides gives way to the next.
  // The note under the header (optional, one line) is saved with the marks when the field loses the focus or on Enter.
  import { signedNumber } from '$lib/format/number'
  import { errorText } from '$lib/format/error'
  import { Button, buttonVariants } from '$lib/components/ui/button'
  import * as DropdownMenu from '$lib/components/ui/dropdown-menu'
  import Marker from '$lib/components/results/Marker.svelte'
  import { saveText } from '$lib/desktop'
  import { massShift, peptideView, psmProforma, type PeptideView } from '$lib/format/peptide'
  import { annotatePeaks, type ModificationMass } from '$lib/psm/annotate'
  import {
    annotationOptions,
    fragmentCharges,
    fragmentTolerance,
    groupByPeak,
    instrumentOf,
    ionTable,
    ladderCuts,
    lossOptions,
    matchedByReference,
    matchSummary,
    modificationList,
    nceFor,
    onePlusHidden,
    precursorOptions,
    predictedIons,
    resolveOptions,
    similarityText,
    spectrumPeaks,
    svgFileName,
    visibleMatches,
    visibleTheoretical,
    withPrecursor,
    ION_TYPES,
    type IonType,
    type PsmOptions,
  } from '$lib/psm/psm'
  import { exportSpectrumSvg } from '$lib/psm/svg-export'
  import { COISOLATED_TITLE, chimericRank, isCoisolated, sameSpectrum } from '$lib/results/coisolated'
  import { ACCEPTED, MAX_NOTE, NONE, REJECTED } from '$lib/results/curation'
  import { baseName } from '$lib/results/model'
  import { formatCount, formatQValue, parseAccession, parseDescription, scanNumber } from '$lib/results/display'
  import type { LoadedSpectra, PsmSpectra } from '$lib/results/read'
  import type { PeakAnnotationResult } from '$lib/results/types'
  import type { ResultsView } from '$lib/results/view.svelte'
  import { defaultsOf } from '$lib/settings/parameters'
  import { PROSE_PARAMETERS } from '$lib/settings/prose'
  import ChevronDown from '@lucide/svelte/icons/chevron-down'
  import XIcon from '@lucide/svelte/icons/x'
  import RulerIcon from '@lucide/svelte/icons/ruler'
  import TagIcon from '@lucide/svelte/icons/tag'
  import { deltaCandidates, NO_MARKS, type PsmMarks, type SpectrumTool } from '$lib/psm/measure'
  import { cn } from '$lib/utils'
  import { displaySettings } from '$lib/format/settings.svelte'
  import { tick } from 'svelte'
  import FragmentLadder from './FragmentLadder.svelte'
  import IonTable from './IonTable.svelte'
  import PsmFeatures from './PsmFeatures.svelte'
  import SpectrumMirror, { type Bottom } from './SpectrumMirror.svelte'

  let {
    view,
    onClose,
    onUp,
  }: {
    view: ResultsView
    /** Closes the view (Esc, ✕): the PSM table gets the room back, the PSM stays selected. */
    onClose: () => void
    /** Up a level: the PSM's peptide in the Peptides tab, or its protein in the Proteins tab. */
    onUp: (to: { peptide: number } | { protein: number }) => void
  } = $props()

  const uid = $props.id()
  const m = $derived(view.model)
  const ps = $derived(m.tables.psms)
  const pt = $derived(m.tables.peptides)
  const row = $derived(view.psm?.row ?? 0)
  /** The open PSM's curation mark (the marks change in place: read with their count of changes). */
  const mark = $derived.by(() => {
    void view.curationVersion
    return view.curation[row]
  })
  /** The open PSM's note ('' none). */
  const note = $derived.by(() => {
    void view.curationVersion
    return view.curationNotes.get(row) ?? ''
  })
  /** Marks the open PSM, or takes the mark away when it has it. */
  function curate(next: typeof ACCEPTED | typeof REJECTED) {
    view.curate(row, mark === next ? NONE : next)
  }
  const k = $derived(ps.peptide[row])
  const charge = $derived(ps.charge[row])
  const sequence = $derived(pt.sequence[k])
  const fixedMods = $derived(view.result.settings.fixedMods)
  const peptide: PeptideView = $derived(peptideView(pt, k, fixedMods))
  const shift = $derived(massShift(ps.deltaMass[row]))
  /** The shift as a number on the page (a real minus); `shift` is the ProForma text. */
  const shiftText = $derived(shift === null ? null : signedNumber(ps.deltaMass[row], 4))
  const scan = $derived(scanNumber(ps.nativeId[row]))
  // (no spectrum reference: ProSE stored no spectrum for the PSM, the reader gives none)
  const spectrumName = $derived(scan !== null ? `scan ${scan}` : ps.nativeId[row] ? `spectrum ${ps.nativeId[row]}` : 'spectrum not referenced')

  // the search's settings: the parameters it changed, else ProSE's defaults
  const parameters = $derived(view.result.settings.parameters)
  const defaults = defaultsOf(PROSE_PARAMETERS)
  const tolerance = $derived(fragmentTolerance(parameters, defaults))
  const instrument = $derived(instrumentOf(parameters, defaults))
  // The peaks and PeptDeep's prediction of the PSM: read from its input's psm file when it is opened
  const source = $derived(view.result.outcome.spectra)
  // ProSE wrote PeptDeep's predictions (none at all: a tool without ONNX Runtime)
  const onnx = $derived(source.predictions)
  // (raw: the spectra are large typed arrays, and `source` is compared by identity)
  let loaded = $state.raw<{ row: number; source: PsmSpectra; data: LoadedSpectra | null; error: string | null } | null>(null)
  $effect(() => {
    const [r, s] = [row, source]
    let current = true
    s.load(r).then(
      (data) => {
        if (current) loaded = { row: r, source: s, data, error: null }
      },
      (e: unknown) => {
        if (current) loaded = { row: r, source: s, data: null, error: errorText(e) }
      },
    )
    return () => {
      current = false
    }
  })
  /** The spectra of this PSM; null while they are read. */
  const spectra = $derived(loaded && loaded.row === row && loaded.source === source ? loaded : null)
  const peaks = $derived(spectra?.data ? spectrumPeaks(spectra.data.spectra, spectra.data.spectrumRow) : null)
  // ProSE stored the spectra filtered (its -qpx_peak_top and -qpx_peak_window, in the footer of the input's mz view)
  const peakFilter = $derived(spectra?.data?.peakFilter ?? null)
  const mods = $derived.by(() => {
    const out: { name: string; residue: string }[] = []
    for (let i = pt.modOffsets[k]; i < pt.modOffsets[k + 1]; i++) out.push({ name: pt.modName[i], residue: sequence[pt.modPosition[i]] ?? '' })
    return out
  })
  /** The masses of the peptide's modifications by name, for a name the Unimod table does not have. */
  const modificationMass: ModificationMass = $derived.by(() => {
    const masses = new Map<string, number>()
    for (let i = pt.modOffsets[k]; i < pt.modOffsets[k + 1]; i++) masses.set(pt.modName[i], pt.modMass[i])
    return (name: string) => masses.get(name)
  })
  const losses = $derived(lossOptions(mods))
  const options: PsmOptions = $derived(resolveOptions(view.psmOptions, { charge, losses, onnx }))
  const engineOptions = $derived(annotationOptions(options, tolerance))
  const nce = $derived(nceFor(view.result.outcome.summary.files[ps.file[row]], peaks?.collisionEnergy ?? NaN, parameters))

  /** Annotates the PSM's peaks (none: the theoretical ions alone); an error as text. */
  function annotate(o: Parameters<typeof annotatePeaks>[4]): { result: PeakAnnotationResult | null; error: string | null } {
    try {
      const mz = peaks?.mz ?? new Float32Array(0)
      const intensity = peaks?.intensity ?? new Float32Array(0)
      return { result: annotatePeaks(mz, intensity, pt.openms[k], charge, o, modificationMass), error: null }
    } catch (e) {
      return { result: null, error: String((e as Error)?.message ?? e) }
    }
  }

  // the annotation (also without peaks: its theoretical ions make the ion table)
  const annotation = $derived(annotate(engineOptions))

  // the precursor peaks, asked for apart: the annotation makes them at `maxCharge` only, which is the
  // precursor's charge here (and not the fragments' 1 or 2)
  const precursorAnnotation = $derived(options.precursor ? annotate(precursorOptions(tolerance, charge)) : null)
  const precursorResult = $derived(precursorAnnotation?.result ?? null)
  /** Why the precursor peaks are missing although they were asked for. */
  const precursorError = $derived(precursorAnnotation?.error ?? null)

  const fragments = $derived(annotation.result)
  const result = $derived(fragments ? withPrecursor(fragments, precursorResult) : null)
  const annotationError = $derived(annotation.error)

  const shown = $derived(result ? visibleMatches(result.matches, options) : [])
  const groups = $derived(result && peaks ? groupByPeak(shown, peaks.mz, peaks.intensity) : [])
  const theoretical = $derived(result ? visibleTheoretical(result.theoretical, options) : [])
  const table = $derived(result ? ionTable(result.theoretical, shown, sequence.length, options) : null)
  const cuts = $derived(ladderCuts(shown, sequence.length))
  const summary = $derived(result && peaks ? matchSummary(shown, theoretical, peaks.intensity) : null)
  /** Nothing matches with 1+ fragments hidden, and 1+ would: a hint with a button that shows them. */
  const hiddenOnePlus = $derived(result !== null && peaks !== null && onePlusHidden(result.matches, options))

  // PeptDeep's prediction for the lower half, as ProSE wrote it for this PSM (at the NCE of its file)
  const prediction = $derived(spectra?.data ? predictedIons(spectra.data.predicted, spectra.data.row) : [])

  const bottom: Bottom = $derived.by(() => {
    if (options.bottom === 'theoretical') {
      if (!result) return { kind: 'status', label: 'Theoretical', message: 'Computing the theoretical spectrum…', failed: false }
      const matched = new Set(shown.map((s) => s.ion))
      return {
        kind: 'ions',
        label: 'Theoretical',
        relative: false,
        ions: theoretical.map((t) => ({ mz: t.mz, height: 0.55, type: t.type, ion: t.ion, faint: !matched.has(t.ion) })),
      }
    }
    const label = `PeptDeep · ${nce.label} · ${instrument} · 100 % = strongest predicted b/y`
    if (spectra === null) return { kind: 'status', label: 'PeptDeep', message: 'Reading the prediction…', failed: false }
    if (prediction.length === 0) {
      return { kind: 'status', label: 'PeptDeep', message: 'ProSE wrote no prediction for this PSM', failed: true }
    }
    const ions = prediction
      .filter((p) => (options.ions as string[]).includes(p.type) && options.charges.includes(p.charge))
      .map((p) => ({ mz: p.mz, height: p.intensity, type: p.type, ion: p.ion, faint: false }))
    return { kind: 'ions', label, relative: true, ions }
  })

  /** PeptDeep's similarity that the search computed (over all ions), whenever the search has it. */
  const similarity = $derived(similarityText(ps.featureNames, spectra?.data?.features ?? null))

  /**
   * The observed half's 100 %: against PeptDeep, the strongest matched b or y peak (as PeptDeep scales
   * its prediction); against the theoretical spectrum, or without a matched b or y, the base peak.
   */
  const reference = $derived.by(() => {
    let base = 0
    for (let i = 0; i < (peaks?.intensity.length ?? 0); i++) base = Math.max(base, peaks!.intensity[i])
    const matched = options.bottom === 'peptdeep' ? matchedByReference(groups, options) : null
    return matched !== null ? { intensity: matched, text: 'strongest matched b/y' } : { intensity: base, text: 'base peak' }
  })

  /** The prediction is on its way: the spectrum is not exported meanwhile. */
  const pending = $derived(bottom.kind === 'status' && !bottom.failed)


  // the ways up and the header: the protein is the one the tables are narrowed to if the peptide is its, else its first
  const proteinRow = $derived(view.proteinOf(k))
  const proteinCrumb = $derived.by(() => {
    if (proteinRow === null) return null
    const accession = m.tables.proteins.accession[proteinRow]
    const gene = parseDescription(m.tables.proteins.description[proteinRow]).gene
    const more = m.peptide.proteins[k].length - 1
    return { text: `${gene ?? parseAccession(accession).id}${more > 0 ? ` +${more}` : ''}`, title: accession }
  })
  /** The peptide in one line of text: "AVM(ox)DDFAAFVEK", after its shift. */
  const peptideCrumb = $derived.by(() => {
    const label = (mod: { label: string } | undefined) => (mod ? `(${mod.label})` : '')
    const core = peptide.residues.map((r) => `${r.residue}${label(r.mod)}`).join('')
    return `${shift ? `[${shift}]?` : ''}${peptide.nTerm ? `(${peptide.nTerm.label})` : ''}${core}${peptide.cTerm ? `(${peptide.cTerm.label})` : ''}`
  })
  const proforma = $derived(psmProforma(pt.proforma[k], ps.deltaMass[row]))
  const fileName = $derived(baseName(m.tables.files[ps.file[row]] ?? ''))

  const headerLine = $derived.by(() => {
    const rt = ps.rt[row]
    const err = ps.precursorErrorPpm[row]
    return [
      m.samples[m.psmSample[row]]?.label,
      `${charge}+`,
      Number.isFinite(ps.mz[row]) ? `m/z ${ps.mz[row].toFixed(4)}` : null,
      rt >= 0 ? `RT ${(rt / 60).toFixed(1)} min` : null,
      `q ${formatQValue(ps.qValue[row])}`,
      // an open search's shift is the whole mass difference: no error is left to show
      Number.isFinite(err) && shift === null ? `${signedNumber(err, 1)} ppm` : null,
    ]
      .filter((p) => p)
      .join(' · ')
  })

  const neighbours = $derived(view.psmNeighbours)

  /** The other PSMs of this spectrum (co-isolated precursors), as links: "VHGEEDPTKK 3+". */
  const mates = $derived(
    sameSpectrum(ps, row).map((j) => {
      const [sequence, rank] = [pt.sequence[ps.peptide[j]], chimericRank(ps, j)]
      const which = rank >= 2 ? ', a co-isolated precursor' : rank === 1 ? ', the precursor the spectrum was taken of' : ''
      return { row: j, sequence, charge: ps.charge[j], coisolated: rank >= 2, decoy: ps.decoy[j] === 1, title: `Open the PSM of ${sequence}, ${ps.charge[j]}+${which}` }
    }),
  )

  const spectrumTitle = $derived.by(() => {
    const listed = modificationList(pt, k, fixedMods)
    const below = options.bottom === 'peptdeep' ? `PeptDeep's prediction at ${nce.label}` : 'the theoretical spectrum'
    return `Annotated spectrum of ${sequence}${listed.length > 0 ? ` (${listed.join(', ')})` : ''}, ${charge}+, ${spectrumName}. Below it is ${below}.`
  })
  const spectrumDescription = $derived(
    peaks
      ? `${groups.length} of ${peaks.mz.length} peaks are matched. b ions are blue, y ions orange, other ions dark grey and unmatched peaks light grey.` +
          (summary ? ` ${summary.matched} of the ${summary.total} ions shown are matched, and their peaks carry ${Math.round(summary.intensityFraction * 100)} % of the intensity.` : '') +
          ` In the upper half, 100 % is the ${reference.text}.` +
          (options.bottom === 'peptdeep' ? ' In the lower half, 100 % is the strongest predicted b or y ion.' : ' In the lower half, every ion has the same height.') +
          (similarity ? ` ${similarity.replace(' (search)', '')}, as the search computed it over all ions.` : '')
      : '',
  )

  /** What the losses menu's button says: the losses on ("CH₄SO, other", "none"). */
  const moreSummary = $derived.by(() => {
    const on = losses.filter((l) => options.losses.includes(l.formula)).map((l) => (l.formula === 'other' ? 'other' : l.label))
    return on.length > 0 ? on.join(', ') : 'none'
  })
  const moreOn = $derived(options.losses.length > 0)
  /** The PSM's precursor, marked in the spectrum at its m/z. */
  const precursorMark = $derived(Number.isFinite(ps.mz[row]) ? { mz: ps.mz[row], charge } : null)

  // the toggles: a choice made here becomes the user's (no longer automatic) for all PSMs
  function setIon(type: IonType, on: boolean) {
    const ions = new Set(options.ions)
    if (on) ions.add(type)
    else ions.delete(type)
    view.psmOptions.ions = ION_TYPES.filter((t) => ions.has(t))
  }
  function setCharge(c: number, on: boolean) {
    const charges = new Set(options.charges)
    if (on) charges.add(c)
    else charges.delete(c)
    view.psmOptions.charges = [...charges].sort()
  }
  function setLoss(formula: string, on: boolean) {
    const chosen = new Set(options.losses)
    if (on) chosen.add(formula)
    else chosen.delete(formula)
    view.psmOptions.losses = [...chosen]
  }

  let mirror = $state<SpectrumMirror | null>(null)
  /** The m/z that the ion table and the spectrum highlight together. */
  let hoverMz = $state<number | null>(null)

  // the tools: one at a time; what they made, per spectrum (the view keeps it for the session)
  let tool = $state<SpectrumTool>(null)
  /** A measurement is half made, or a label being written (the mirror's, while there is one). */
  let toolBusy = $state(false)
  // (another PSM: the mirror goes while its peaks are read, and with it what was half made)
  $effect.pre(() => {
    void row
    toolBusy = false
    // (the m/z the ion table and the spectrum shared was the last PSM's)
    hoverMz = null
  })
  // (a PSM without stored peaks has nothing to measure: the tool goes, so that Esc closes the view)
  $effect.pre(() => {
    if (spectra !== null && !peaks) tool = null
  })
  /** Whose marks these are: the spectrum's (its file and native ID; the PSM's, when the spectrum is not referenced). */
  const marksKey = $derived(ps.nativeId[row] ? `${ps.file[row]} ${ps.nativeId[row]}` : `psm ${row}`)
  const marks = $derived(view.psmMarks[marksKey] ?? NO_MARKS)
  function setMarks(next: PsmMarks) {
    view.psmMarks[marksKey] = next
  }
  const stringsOf = (v: unknown): string[] => (Array.isArray(v) ? v.filter((x): x is string => typeof x === 'string') : [])
  /**
   * What a Δm/z may be: the residues, with the search's fixed and variable modifications; at 1+, and 2+ while 2+
   * fragments are shown.
   */
  const measure = $derived({
    tolerance,
    candidates: deltaCandidates(
      stringsOf(parameters['modifications:fixed'] ?? defaults['modifications:fixed']),
      stringsOf(parameters['modifications:variable'] ?? defaults['modifications:variable']),
    ),
    charges: options.charges.includes(2) ? [1, 2] : [1],
  })
  function setTool(next: SpectrumTool) {
    tool = tool === next ? null : next
  }
  const toolPending = $derived(toolBusy && mirror !== null)
  const toolHint = $derived(
    tool === 'measure'
      ? toolPending
        ? 'Click the second peak · Esc cancels'
        : 'Click two peaks for their Δm/z'
      : tool === 'label'
        ? toolPending
          ? 'Enter keeps the label · Esc cancels'
          : 'Click a peak to label it'
        : '',
  )
  function exportSvg() {
    const svg = mirror?.svgElement()
    if (!svg) return
    void saveText(svgFileName(proforma, ps.nativeId[row]), exportSpectrumSvg(svg))
  }

  // the Export menu's "Spectrum (SVG)" does what the button does
  $effect(() => {
    view.spectrumExporter = peaks && !pending ? exportSvg : null
    return () => (view.spectrumExporter = null)
  })

  let ladderRow = $state(0)
  /** The height the spectrum's drawing may take: what the view leaves between the toolbar and its lower edge. */
  let plotRoom = $state(0)
  /**
   * With less room than this the fragment mass errors give way to the spectrum (the toggle says so): the halves of
   * the mirror would get lower than some 60 px, and the labels of a crowded spectrum have no room above the peaks.
   * The default window (1440 × 900, its page some 875 px high under the menu bar, four PSMs in the table above)
   * leaves about 350 px: the errors show (as a lower strip), with nearly as many ions labelled as in a plot of the
   * height it has by itself, and stay when a line more of the header takes some of the room.
   */
  const ROOM_FOR_ERRORS = 300
  const errorsFit = $derived(plotRoom === 0 || plotRoom >= ROOM_FOR_ERRORS)
  /** The column beside the spectrum: the PSM's features, or the ion table. */
  let side = $state<'features' | 'fragments'>('features')
  /**
   * The column shows where the user asked for it, and unasked only in a view wide enough for it and a spectrum as
   * wide as the labels of a crowded spectrum need (in a narrower plot some would be left out). In a narrow view
   * it lies over the spectrum.
   */
  let viewWidth = $state(0)
  const SIDE_BY_ITSELF = 1560
  const SIDE_BESIDE = 900
  const sideShown = $derived(displaySettings.psmSide ?? viewWidth >= SIDE_BY_ITSELF)
  const sideOver = $derived(viewWidth < SIDE_BESIDE)
  let heading = $state<HTMLElement | null>(null)
  let prevButton = $state<HTMLElement | null>(null)
  let nextButton = $state<HTMLElement | null>(null)
  /** Moves the keyboard focus into the view (to its heading, which names the PSM). */
  export function focus(): void {
    heading?.focus()
  }

  /** Previous or next PSM; the focus stays on the button, or goes to the other one when this one is done. */
  async function step(direction: 1 | -1) {
    const from = document.activeElement
    view.stepPsm(direction)
    await tick()
    if (from === prevButton || from === nextButton) {
      const own = direction < 0 ? prevButton : nextButton
      const other = direction < 0 ? nextButton : prevButton
      if (own && !(own as HTMLButtonElement).disabled) own.focus()
      else other?.focus()
    }
  }

  /**
   * Keys that belong to what has the focus: typing, a grid, radio buttons, menus. Not the PSM table above the view:
   * the focus is there while the user goes through the PSMs, its rows are the view's PSMs, and it takes only the
   * keys that move through the rows (↑ ↓, Home, End, the page keys, Enter, Backspace), so the view's own (A, R, M, L,
   * ← →, Delete) work from it too.
   */
  function ownsKeys(target: EventTarget | null): boolean {
    if (!(target instanceof Element)) return false
    if (target.closest('input, textarea, select, [contenteditable="true"]')) return true
    if (target.closest('[data-testid="results-table-area"] [role="grid"]')) return false
    return target.closest('[role="grid"], [role="tablist"], [role="menu"], [role="listbox"], [role="dialog"]') !== null
  }

  function onkeydown(event: KeyboardEvent) {
    if (event.defaultPrevented || event.altKey || event.ctrlKey || event.metaKey || event.shiftKey) return
    if (event.key === 'Escape') {
      if (event.target instanceof Element && event.target.closest('input[type="search"], input[type="text"], textarea, [role="dialog"], [role="menu"]')) return
      event.preventDefault()
      // what is half done (or selected) first, then the tool, then the view
      if (mirror?.cancel()) return
      if (tool) tool = null
      else onClose()
    } else if (event.key === 'Delete' || event.key === 'Backspace') {
      if (ownsKeys(event.target)) return
      if (mirror?.deleteSelection()) event.preventDefault()
    } else if (event.key === 'ArrowLeft' || event.key === 'ArrowRight') {
      if (ownsKeys(event.target)) return
      event.preventDefault()
      void step(event.key === 'ArrowLeft' ? -1 : 1)
    } else if (event.key === 'a' || event.key === 'A' || event.key === 'r' || event.key === 'R') {
      if (ownsKeys(event.target) || event.repeat) return
      event.preventDefault()
      curate(event.key.toLowerCase() === 'a' ? ACCEPTED : REJECTED)
    } else if (event.key === 'm' || event.key === 'M' || event.key === 'l' || event.key === 'L') {
      if (ownsKeys(event.target) || !peaks) return
      event.preventDefault()
      setTool(event.key.toLowerCase() === 'm' ? 'measure' : 'label')
    }
  }

  const toggle =
    'inline-flex h-[26px] min-w-[30px] cursor-pointer items-center justify-center rounded-md border bg-background px-2 text-xs text-muted-foreground select-none hover:text-foreground peer-checked:border-primary/50 peer-checked:bg-primary/10 peer-checked:font-medium peer-checked:text-primary peer-focus-visible:ring-2 peer-focus-visible:ring-ring/50 peer-disabled:cursor-not-allowed peer-disabled:opacity-50'
  const toolButton =
    'inline-flex h-[26px] cursor-pointer items-center justify-center gap-1 rounded-md border bg-background px-2 text-xs text-muted-foreground select-none hover:text-foreground focus-visible:ring-2 focus-visible:ring-ring/50 focus-visible:outline-none disabled:cursor-not-allowed disabled:opacity-50 disabled:hover:text-muted-foreground'
  const toolOn = 'border-primary/50 bg-primary/10 font-medium text-primary hover:text-primary'
  const crumb = 'rounded-sm hover:text-foreground hover:underline focus-visible:ring-2 focus-visible:ring-ring/50 focus-visible:outline-none'
</script>

<svelte:window {onkeydown} />

<section class="@container/psm flex min-h-0 min-w-0 flex-1 flex-col gap-1.5 pt-0.5" aria-labelledby="{uid}-heading" data-testid="psm-view" bind:clientWidth={viewWidth}>
  <!-- The peptide with its ladder and what the PSM is take what the buttons leave them: in a narrow view the buttons
       go to a row of their own. -->
  <header class="flex flex-none flex-wrap items-center justify-between gap-x-3 gap-y-1">
    <div class="flex min-w-0 flex-[1_1_18rem] items-end" bind:clientWidth={ladderRow}>
      <FragmentLadder {peptide} {cuts} {shift} available={ladderRow} />
    </div>
    <div class="ml-auto flex flex-none flex-wrap items-center justify-end gap-1.5">
      {#if view.curationError}
        <span class="max-w-60 truncate text-xs text-destructive" role="alert" title={view.curationError}>{view.curationError}</span>
      {/if}
      <div class="flex items-center gap-1" role="group" aria-label="Curation" data-testid="psm-curation">
        <Button
          variant="outline"
          size="sm"
          class={cn('h-7 px-2.5 text-xs', mark === ACCEPTED && 'border-emerald-600 bg-emerald-600 text-white hover:bg-emerald-700 hover:text-white')}
          aria-pressed={mark === ACCEPTED}
          aria-keyshortcuts="A"
          title={mark === ACCEPTED ? 'Accepted. Take the mark away (A)' : 'Accept the PSM (A)'}
          onclick={() => curate(ACCEPTED)}>✓ Accept</Button>
        <Button
          variant="outline"
          size="sm"
          class={cn('h-7 px-2.5 text-xs', mark === REJECTED && 'border-destructive bg-destructive text-white hover:bg-destructive/90 hover:text-white')}
          aria-pressed={mark === REJECTED}
          aria-keyshortcuts="R"
          title={mark === REJECTED ? 'Rejected. Take the mark away (R)' : 'Reject the PSM (R)'}
          onclick={() => curate(REJECTED)}>✗ Reject</Button>
      </div>
      <!-- its place among the rows the PSM table shows, which Prev and Next (and the table's arrow keys) go through -->
      <span
        class="pl-1 text-xs whitespace-nowrap text-muted-foreground tabular-nums"
        data-testid="detail-position"
        title={neighbours.position < 0 ? "The table's filters hide this PSM" : `Row ${formatCount(neighbours.position + 1)} of the ${formatCount(neighbours.count)} the PSM table shows`}
        >{neighbours.position < 0 ? 'not in the table' : `${formatCount(neighbours.position + 1)} of ${formatCount(neighbours.count)}`}</span>
      <Button
        bind:ref={prevButton}
        variant="outline"
        size="sm"
        class="h-7 px-2.5 text-xs"
        aria-label="Previous PSM"
        title={neighbours.position < 0 ? "The table's filters hide this PSM" : 'Previous PSM in the table (←)'}
        aria-keyshortcuts="ArrowLeft"
        disabled={neighbours.prev === null}
        onclick={() => step(-1)}>‹ Prev</Button>
      <Button
        bind:ref={nextButton}
        variant="outline"
        size="sm"
        class="h-7 px-2.5 text-xs"
        aria-label="Next PSM"
        title={neighbours.position < 0 ? "The table's filters hide this PSM" : 'Next PSM in the table (→)'}
        aria-keyshortcuts="ArrowRight"
        disabled={neighbours.next === null}
        onclick={() => step(1)}>Next ›</Button>
      <Button
        variant="outline"
        size="sm"
        class="h-7 px-2.5 text-xs"
        disabled={!peaks || pending}
        title={spectra === null ? 'Reading the spectrum' : !peaks ? 'The spectrum was not stored' : pending ? 'Wait for the annotation and the prediction' : undefined}
        onclick={exportSvg}>Export spectrum (SVG)</Button>
      <!-- the column of the features and the ion table, beside the spectrum (which gets narrower) -->
      <Button
        variant="outline"
        size="sm"
        class={cn('h-7 px-2.5 text-xs', sideShown && 'border-primary/50 bg-primary/10 text-primary hover:text-primary')}
        aria-expanded={sideShown}
        aria-controls="{uid}-side"
        data-testid="psm-side-toggle"
        title={sideShown ? "Hide the PSM's features and its ion table: the spectrum gets the width" : "Show the PSM's features and its ion table beside the spectrum"}
        onclick={() => (displaySettings.psmSide = !sideShown)}>Features · Fragments</Button>
      <Button variant="ghost" size="icon-sm" aria-label="Close the PSM view" title="Close (Esc)" onclick={onClose}>
        <XIcon aria-hidden="true" />
      </Button>
    </div>
  </header>

  <!-- what the PSM is, the ways up (Backspace in the PSM table goes to the peptide), the note, the other PSMs of
       the spectrum: one row where the view is wide enough -->
  <div class="flex flex-none flex-wrap items-center gap-x-4 gap-y-1 text-xs">
    <div class="flex min-w-0 max-w-full flex-wrap items-baseline gap-x-2 text-muted-foreground">
      <h2
        bind:this={heading}
        id="{uid}-heading"
        tabindex="-1"
        class="max-w-full min-w-0 truncate rounded-sm font-normal text-foreground outline-none focus-visible:ring-2 focus-visible:ring-ring/50"
        title="PSM in {fileName}, {spectrumName}">
        PSM in {fileName}, {spectrumName}
      </h2>
      <p class="tabular-nums" data-testid="psm-header">
        {#if shift}<span class="font-mono">[{shift}]?</span>{' · '}{/if}{headerLine}{#if ps.decoy[row]}<Marker text="decoy" />{/if}{#if isCoisolated(ps, row)}<Marker
            text="co-isolated"
            title={COISOLATED_TITLE} />{/if}
      </p>
      <!-- the focus stays in the PSM table above: say which PSM opened -->
      <p class="sr-only" aria-live="polite">PSM {peptideCrumb}, {charge}+, {spectrumName} ({fileName})</p>
    </div>
    <nav aria-label="Up a level" class="flex min-w-0 items-baseline gap-x-1.5 whitespace-nowrap text-muted-foreground" data-testid="psm-up">
      <span>Up to</span>
      <button type="button" class="{crumb} max-w-[20rem] min-w-0 truncate font-medium text-primary" title="The Peptides tab at {proforma} (Backspace in the table)" onclick={() => onUp({ peptide: k })}
        >peptide {peptideCrumb} ↑</button>
      <span aria-hidden="true">·</span>
      {#if proteinCrumb && proteinRow !== null}
        <button type="button" class="{crumb} min-w-0 truncate font-medium text-primary" title="The Proteins tab at {proteinCrumb.title}" onclick={() => onUp({ protein: proteinRow! })}
          >protein {proteinCrumb.text} ↑</button>
        <span class="sr-only">({proteinCrumb.title})</span>
      {:else}
        <span class="min-w-0 truncate italic" title="None of this peptide's proteins pass the protein FDR">below protein FDR</span>
      {/if}
    </nav>
    {#key row}
      <label class="flex min-w-[12rem] flex-[1_1_14rem] items-center gap-2" data-testid="psm-note">
        <span class="text-muted-foreground">Note</span>
        <input
          type="text"
          class="h-7 min-w-0 flex-1 rounded-md border bg-background px-2 text-xs outline-none placeholder:text-muted-foreground focus-visible:border-ring focus-visible:ring-2 focus-visible:ring-ring/50"
          placeholder="Optional, e.g. why you accepted or rejected the PSM"
          maxlength={MAX_NOTE}
          spellcheck="false"
          value={note}
          onchange={(e) => view.setNote(row, e.currentTarget.value)}
          onkeydown={(e) => e.key === 'Enter' && e.currentTarget.blur()} />
      </label>
    {/key}
    {#if mates.length > 0}
      <!-- the PSMs of the other precursors of this spectrum: each opens here (the table above keeps its place) -->
      <p class="flex flex-wrap items-baseline gap-x-2 gap-y-0.5 text-muted-foreground" data-testid="same-spectrum">
        <span>Same spectrum:</span>
        {#each mates as mate (mate.row)}
          <span class="whitespace-nowrap">
            <button type="button" class="{crumb} font-mono text-foreground" title={mate.title} onclick={() => view.openPsm(mate.row)}
              >{mate.sequence}</button>
            {mate.charge}+{#if mate.decoy}{' '}decoy{/if}{#if mate.coisolated}{' '}co-isolated{/if}
          </span>
        {/each}
      </p>
    {/if}
  </div>
  {#if shift}
    <p class="flex-none truncate text-xs text-muted-foreground" data-testid="open-search-note" title="This PSM from the open search has a mass shift of {shiftText} Da at an unknown position. The annotation shows the peptide without the shift, so fragments that carry it are not annotated.">
      This PSM from the open search has a mass shift of {shiftText} Da at an unknown position. The annotation shows the peptide
      without the shift, so fragments that carry it are not annotated.
    </p>
  {/if}

  <!-- the spectrum (it takes the height that is left) and, beside it, the column of the features and the ion table.
       The row never gets lower than its toolbar and the least plot: in a window too low for that, the pane the view
       is in scrolls (what fills the row is out of the flow, so that only those two count) -->
  <div class="relative flex flex-1 gap-3">
  <div class="flex min-w-0 flex-1 flex-col rounded-lg border px-3.5 pt-2.5 pb-1.5">
    <div class="mb-1 flex flex-none flex-wrap items-center gap-x-4 gap-y-1.5 text-xs">
      <div class="flex items-center gap-1" role="group" aria-labelledby="{uid}-ions">
        <span id="{uid}-ions" class="mr-1 text-muted-foreground">Ions</span>
        {#each ION_TYPES as t (t)}
          <label class="relative">
            <input
              type="checkbox"
              class="peer absolute inset-0 z-10 m-0 cursor-pointer opacity-0 disabled:cursor-not-allowed"
              aria-label="{t} ions"
              checked={options.ions.includes(t)}
              onchange={(e) => setIon(t, e.currentTarget.checked)} />
            <span class={toggle}>{t}</span>
          </label>
        {/each}
      </div>
      <div class="flex items-center gap-1" role="group" aria-labelledby="{uid}-charge">
        <span id="{uid}-charge" class="mr-1 text-muted-foreground">Charge</span>
        <!-- a 1+ precursor has 1+ fragments only -->
        {#each fragmentCharges(charge) as c (c)}
          <label class="relative">
            <input
              type="checkbox"
              class="peer absolute inset-0 z-10 m-0 cursor-pointer opacity-0 disabled:cursor-not-allowed"
              aria-label="{c}+ fragments"
              checked={options.charges.includes(c)}
              onchange={(e) => setCharge(c, e.currentTarget.checked)} />
            <span class={toggle}>{c}+</span>
          </label>
        {/each}
      </div>
      <label class="relative" title={errorsFit ? 'The fragment mass errors under the spectrum' : 'The fragment mass errors under the spectrum. The view is too low for them now: drag the line above it up.'}>
        <input
          type="checkbox"
          class="peer absolute inset-0 z-10 m-0 cursor-pointer opacity-0"
          aria-label="Mass errors"
          data-testid="mass-errors-toggle"
          checked={displaySettings.massErrors}
          onchange={(e) => (displaySettings.massErrors = e.currentTarget.checked)} />
        <!-- (on, with no room for them: the toggle is dimmed, its tooltip says how to get them) -->
        <span class={cn(toggle, displaySettings.massErrors && !errorsFit && 'opacity-55')} data-testid="mass-errors-label">mass errors</span>
      </label>
      <label class="relative" title="The precursor peaks, with their losses of water and ammonia (the precursor's m/z is marked in the spectrum always)">
        <input
          type="checkbox"
          class="peer absolute inset-0 z-10 m-0 cursor-pointer opacity-0 disabled:cursor-not-allowed"
          aria-label="Precursor peaks"
          data-testid="precursor-toggle"
          checked={options.precursor}
          onchange={(e) => (view.psmOptions.precursor = e.currentTarget.checked)} />
        <span class={toggle}>precursor</span>
      </label>
      <!-- the tools on the observed peaks (one at a time) and what clears what they made -->
      <div class="flex items-center gap-1" role="group" aria-label="Tools" data-testid="spectrum-tools">
        <button
          type="button"
          class={cn(toolButton, tool === 'measure' && toolOn)}
          aria-pressed={tool === 'measure'}
          title="Measure (M): click two peaks for their Δm/z (and the residue it may be); a click on a measurement selects it, Delete removes it"
          disabled={!peaks}
          data-testid="tool-measure"
          onclick={() => setTool('measure')}><RulerIcon class="size-3.5" aria-hidden="true" />Measure</button>
        <button
          type="button"
          class={toolButton}
          disabled={marks.measurements.length === 0}
          title="Remove the measurements of this spectrum"
          data-testid="clear-measurements"
          onclick={() => setMarks({ ...marks, measurements: [] })}>Clear Δ</button>
        <button
          type="button"
          class={cn(toolButton, tool === 'label' && toolOn)}
          aria-pressed={tool === 'label'}
          title="Label (L): click a peak to give it a label of your own; a click on a label selects it, Delete removes it, a double click edits it"
          disabled={!peaks}
          data-testid="tool-label"
          onclick={() => setTool('label')}><TagIcon class="size-3.5" aria-hidden="true" />Label</button>
        <button
          type="button"
          class={toolButton}
          disabled={marks.labels.length === 0}
          title="Remove your labels of this spectrum"
          data-testid="clear-labels"
          onclick={() => setMarks({ ...marks, labels: [] })}>Clear labels</button>
        <!-- (as wide as the longest hint while a tool is on: the hint changes between the clicks, the toolbar does not) -->
        {#if toolHint}<span class="ml-1 inline-block w-56 truncate text-muted-foreground" title={toolHint} data-testid="tool-hint">{toolHint}</span>{/if}
      </div>
      <!-- the neutral losses: one menu, its button says what is on -->
      <div class="flex items-center gap-1">
        <span id="{uid}-more" class="mr-1 text-muted-foreground">Losses</span>
        <DropdownMenu.Root>
          <DropdownMenu.Trigger
            class={cn(buttonVariants({ variant: 'outline', size: 'sm' }), 'h-[26px] gap-1 px-2 text-xs font-normal', moreOn && 'border-primary')}
            aria-label="Neutral losses: {moreSummary}">
            {moreSummary}<ChevronDown class="size-3.5 text-muted-foreground" aria-hidden="true" />
          </DropdownMenu.Trigger>
          <DropdownMenu.Content class="w-auto min-w-56" align="start">
            <DropdownMenu.Group>
              <DropdownMenu.GroupHeading class="text-xs font-medium text-muted-foreground">Neutral losses</DropdownMenu.GroupHeading>
              {#each losses as l (l.formula)}
                <DropdownMenu.CheckboxItem
                  checked={options.losses.includes(l.formula)}
                  onCheckedChange={(on) => setLoss(l.formula, on)}
                  title={l.formula === 'other' ? 'Other neutral losses the engine finds, such as losses from side chains' : undefined}
                  >{l.formula === 'other' ? 'other losses' : `${l.label} loss${l.note ? ` (${l.note})` : ''}`}</DropdownMenu.CheckboxItem>
              {/each}
            </DropdownMenu.Group>
          </DropdownMenu.Content>
        </DropdownMenu.Root>
      </div>
      {#if peaks && peakFilter}
        <!-- which peaks the search kept: in the toolbar's free room (beside "Bottom", where the tools fill the first
             line), so that it takes no line of the spectrum's height; cut only where a whole line is too short for it -->
        <p class="min-w-0 flex-[0_1_auto] truncate text-muted-foreground" data-testid="peak-filter-note" title="Showing the top {peakFilter.top} peaks per {peakFilter.window} m/z, plus every peak ProSE annotated.">
          Showing the top {peakFilter.top} peaks per {peakFilter.window} m/z, plus every peak ProSE annotated.
        </p>
      {/if}
      <div class="ml-auto flex items-center gap-1" role="radiogroup" aria-labelledby="{uid}-bottom">
        <span id="{uid}-bottom" class="mr-1 text-muted-foreground">Bottom</span>
        <label class="relative" title={onnx ? undefined : 'This search has no PeptDeep predictions, because ProSE ran without ONNX Runtime'}>
          <input
            type="radio"
            name="{uid}-bottom"
            class="peer absolute inset-0 z-10 m-0 cursor-pointer opacity-0 disabled:cursor-not-allowed"
            value="peptdeep"
            disabled={!onnx}
            checked={options.bottom === 'peptdeep'}
            onchange={() => (view.psmOptions.bottom = 'peptdeep')} />
          <span class={toggle}>PeptDeep prediction</span>
        </label>
        <label class="relative">
          <input
            type="radio"
            name="{uid}-bottom"
            class="peer absolute inset-0 z-10 m-0 cursor-pointer opacity-0 disabled:cursor-not-allowed"
            value="theoretical"
            checked={options.bottom === 'theoretical'}
            onchange={() => (view.psmOptions.bottom = 'theoretical')} />
          <span class={toggle}>Theoretical</span>
        </label>
      </div>
    </div>

    <!-- what is left of the height: the drawing fits it (and never scrolls); the texts that stand in for a spectrum
         scroll if they have to -->
    <div class="relative min-h-[170px] flex-1 {peaks ? 'overflow-hidden' : 'overflow-y-auto'}" bind:clientHeight={plotRoom} data-testid="spectrum-room">
    {#if peaks}
      <div class="absolute inset-0">
      <SpectrumMirror
        bind:this={mirror}
        fit={plotRoom > 0 ? plotRoom : null}
        mz={peaks.mz}
        intensity={peaks.intensity}
        {groups}
        {reference}
        {bottom}
        {similarity}
        title={spectrumTitle}
        description={spectrumDescription}
        errors={displaySettings.massErrors && errorsFit ? { matches: shown, tolerance } : null}
        bind:hover={hoverMz}
        onTheoretical={() => (view.psmOptions.bottom = 'theoretical')}
        {tool}
        {marks}
        onmarks={setMarks}
        {measure}
        precursor={precursorMark}
        bind:pending={toolBusy} />
      </div>
    {:else if spectra === null}
      <div class="my-2 rounded-md border border-dashed px-4 py-8 text-center text-xs text-muted-foreground" data-testid="spectrum-loading">
        Reading the spectrum…
      </div>
    {:else if spectra.error}
      <div class="my-2 rounded-md border border-dashed px-4 py-8 text-center" role="alert" data-testid="spectrum-error">
        <p class="font-medium">The spectrum could not be read</p>
        <p class="mx-auto mt-1 max-w-[44rem] text-xs text-muted-foreground">{spectra.error}</p>
      </div>
    {:else}
      <div class="my-2 rounded-md border border-dashed px-4 py-8 text-center" data-testid="spectrum-missing">
        <p class="font-medium">Spectrum not stored</p>
        {#if ps.nativeId[row] === ''}
          <!-- (ProSE leaves the spectrum reference out when it stored no spectrum for the PSM, e.g. for a second
               spectrum of a native ID) -->
          <p class="mx-auto mt-1 max-w-[44rem] text-xs text-muted-foreground" data-testid="spectrum-not-referenced">
            ProSE did not store a spectrum for this PSM. This happens, for example, when another spectrum of the run has
            the same native ID.
          </p>
        {/if}
        <p class="mx-auto mt-1 max-w-[44rem] text-xs text-muted-foreground">
          The result has no peaks for this PSM, so its spectrum cannot be shown, annotated or exported. A search saves the spectra of
          its PSMs within its PSM FDR. Results opened without them, such as the debug results, have none. The fragment ladder
          needs the peaks too, but the ion table (Fragments) lists the theoretical ions.
        </p>
      </div>
    {/if}
    </div>
    {#if peaks}
      {#if hiddenOnePlus}
        <p class="flex flex-wrap items-center gap-2 pb-1 text-xs text-muted-foreground" role="status" data-testid="oneplus-hint">
          No fragment peaks match. 1+ fragments are hidden.
          <Button variant="outline" size="sm" class="h-6 px-2 text-xs" onclick={() => setCharge(1, true)}>Show 1+</Button>
        </p>
      {/if}
      {#if annotationError}
        <p class="pb-1 text-xs text-destructive" role="alert">The spectrum could not be annotated: {annotationError}</p>
      {:else if precursorError}
        <p class="pb-1 text-xs text-muted-foreground" role="status" data-testid="precursor-note">
          The precursor peaks could not be annotated ({precursorError.length > 90 ? `${precursorError.slice(0, 89)}…` : precursorError}), so the
          fragments are shown without them. To try again, switch Precursor peaks off and on.
        </p>
      {/if}
    {/if}
  </div>

  <!-- beside the spectrum: the PSM's features and the ion table, one at a time (see `sideShown`) -->
  <aside
    id="{uid}-side"
    aria-label="Features and fragments of the PSM"
    data-testid="psm-side"
    class={cn(
      'relative w-[330px] max-w-full flex-none',
      !sideShown ? 'hidden' : sideOver ? 'absolute inset-y-0 right-0 z-10 rounded-lg border bg-background shadow-lg' : '',
    )}>
    <!-- (out of the flow: the column is as high as the spectrum beside it, and its list scrolls) -->
    <div class={cn('absolute inset-0 flex min-h-0 flex-col', sideOver && 'p-3')}>
    <div class="mb-2 flex flex-none items-center gap-1 border-b" role="tablist" aria-label="Features or fragments">
      {#each [['features', 'Features'], ['fragments', 'Fragments']] as const as [value, label] (value)}
        <button
          type="button"
          role="tab"
          id="{uid}-side-{value}"
          aria-selected={side === value}
          aria-controls="{uid}-side-panel"
          class="-mb-px border-b-2 px-2.5 pb-1.5 text-[13px] focus-visible:ring-2 focus-visible:ring-ring/50 focus-visible:outline-none {side === value ? 'border-primary font-medium text-foreground' : 'border-transparent text-muted-foreground hover:text-foreground'}"
          data-testid="psm-side-{value}"
          onclick={() => (side = value)}>{label}</button>
      {/each}
    </div>
    <div id="{uid}-side-panel" role="tabpanel" aria-labelledby="{uid}-side-{side}" class="min-h-0 flex-1 overflow-auto pr-1">
      {#if side === 'features'}
        <PsmFeatures model={m} {row} {fixedMods} matched={summary} features={spectra?.data?.features ?? null} heading={false} />
      {:else if table}
        <IonTable {table} {peptide} label={sequence} bind:hover={hoverMz} />
      {:else}
        <p class="px-2 py-6 text-center text-xs text-muted-foreground">
          {annotationError ? `No ion table: ${annotationError}` : 'Computing the fragment ions…'}
        </p>
      {/if}
    </div>
    </div>
  </aside>
  </div>
</section>
