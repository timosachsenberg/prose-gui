<script lang="ts" module>
  /** An ion of the lower half: PeptDeep's predicted intensity, or the theoretical spectrum (one height). */
  export type BottomIon = { mz: number; height: number; type: string; ion: string; faint: boolean }

  /** The lower half: ions, or what is said instead (predicting, prediction unavailable). */
  export type Bottom =
    | { kind: 'ions'; label: string; ions: BottomIon[]; relative: boolean }
    | { kind: 'status'; label: string; message: string; failed: boolean }
</script>

<script lang="ts">
  // The annotated spectrum, mirrored (mockup 3): the observed peaks above the axis, matched ones in the
  // colour of their series with their ions as direct labels (b blue, y orange, other series dark grey;
  // unmatched peaks light grey); below the axis PeptDeep's prediction or the theoretical spectrum.
  //
  // Both halves say what their 100 % is: against PeptDeep, the observed half is scaled to its strongest
  // matched b or y peak (PeptDeep scales to its strongest b or y ion), and taller peaks are clipped at
  // 100 % with a small marker; against the theoretical spectrum, to the base peak.
  //
  // Drawn by hand: each series is one path, so thousands of peaks stay a handful of nodes; labels are
  // placed most intense first and left out where they would collide. Hovering tells a peak's m/z,
  // intensity and ions; the ion table is the chart's table view. What only the page shows (a prediction
  // on its way) is marked `data-no-export`.
  //
  // Below the lower half, on the same m/z axis (one x scale, the same margins, so each dot sits under its peak), the
  // strip of the fragment mass errors: one dot per annotated ion shown (a/b/c, x/y/z, at every charge shown, with the
  // neutral losses shown, and the precursor peaks when shown; in their colours of the spectrum), its error against the
  // ion's theoretical m/z (psm/annotate.ts) in the unit of the search's fragment tolerance (ppm, or Da), its area by its
  // peak's intensity, the band the tolerance; the m/z labels come below it.
  // It is part of the SVG, so "Export spectrum (SVG)" has it too.
  //
  // The precursor (`precursor`, the PSM's m/z and charge) is marked where it is on the m/z axis, whether a peak is there
  // or not: a dashed line through the upper half, a triangle and "precursor 2+" at its top, which the ions' labels keep
  // clear of; the m/z axis reaches it. Hovering the triangle tells its m/z. It is part of the export.
  //
  // The tools (`tool`, the PSM view's toolbar; psm/measure.ts, as in pyopenms-viewer and openms-viewer): with
  // "Measure", a click snaps to the nearest peak above the axis and marks it, the pointer then draws a dotted line to the
  // peak it would snap to with the live Δm/z, and a second click keeps the measurement (`marks`, through `onmarks`): an
  // orange bracket above the higher peak with its Δm/z and what it may be ("≈ Y"), drawn over the labels: it goes up
  // above the labels it would meet and hides none, its dotted lines start above the labels of their peaks. With "Label",
  // a click on a peak opens a text field there (Enter keeps, Esc cancels; an empty text removes the peak's label); the
  // labels stand above their peaks in a colour of their own, placed before the ions' labels (which then keep clear of
  // them, also of one that found no room and stands right above its peak all the same). A click on a bracket or a label
  // selects it, a second click deselects it (Delete removes it: the caller's keys, `deleteSelection`), a double click on
  // a label edits it; with a tool on, a click at the top of a peak is the tool's, even under a bracket (not under a
  // label of the user's: that is where it is clicked). The measurements and labels are part of the SVG and its export;
  // the pointer's marks and the selection (an overlay) are not.
  import { signedNumber } from '$lib/format/number'
  import {
    bracketAt,
    deltaText,
    labelIndexAt,
    layoutBrackets,
    measurementText,
    snapPeak,
    withLabel,
    NO_MARKS,
    type Box,
    type DeltaContext,
    type PeakPoint,
    type PsmMarks,
    type SpectrumTool,
  } from '$lib/psm/measure'
  import { untrack } from 'svelte'
  import type { PeakMatch } from '$lib/results/types'
  import { errorStats, errorTickDigits, fragmentError, ionColor, ionLabel, COLORS, type PeakGroup, type Tolerance } from '$lib/psm/psm'
  import { clipMarks, mzDomain, nearest, niceTicks, placeLabels, placeLegend, px, scale, stickGroups, stickPath, stripFits, textWidth, wrapParts } from '$lib/psm/plot'

  let {
    mz,
    intensity,
    groups,
    reference,
    bottom,
    similarity = null,
    title,
    description,
    onTheoretical,
    errors = null,
    tool = null,
    marks = NO_MARKS,
    onmarks = () => {},
    measure = null,
    precursor = null,
    pending = $bindable(false),
    hover = $bindable(null),
  }: {
    mz: Float32Array
    intensity: Float32Array
    /** The matched peaks with their ions (`groupByPeak`). */
    groups: PeakGroup[]
    /** The observed half's 100 %: an intensity, and what it is ("strongest matched b/y", "base peak"). */
    reference: { intensity: number; text: string }
    bottom: Bottom
    /** "cosine 0.931 · spectral angle 0.762 (search)", from the PSM's features. */
    similarity?: string | null
    title: string
    description: string
    /** Offered when the prediction failed: switch to the theoretical spectrum. */
    onTheoretical?: () => void
    /** The fragment mass errors under the spectrum: the matches shown and the search's fragment tolerance. */
    errors?: { matches: PeakMatch[]; tolerance: Tolerance } | null
    /** The tool that clicks on the observed peaks use. */
    tool?: SpectrumTool
    /** The user's measurements and labels of this spectrum. */
    marks?: PsmMarks
    /** Keeps changed `marks`. */
    onmarks?: (next: PsmMarks) => void
    /** What a Δm/z may be: the search's fragment tolerance, the masses, the charges (psm/measure.ts); null: the Δ alone. */
    measure?: DeltaContext | null
    /** The PSM's precursor: its m/z and charge; null: not marked. */
    precursor?: { mz: number; charge: number } | null
    /** Out: a measurement is half made or a label is being written (Esc cancels it). */
    pending?: boolean
    /** The m/z highlighted together with the ion table (a matched peak under the pointer, or an ion cell's). */
    hover?: number | null
  } = $props()

  const uid = $props.id()
  /** The plot's own height; the strip below it has the bottom label and, if it does not fit beside it, a line for the similarity. */
  const plotHeight = 320
  const M = { left: 46, right: 14 }
  /** The lines of text above the plot, under it and over the error strip are 15 px apart. */
  const line = 15
  let width = $state(800)

  /** What the upper half is, at the top left. */
  const topLabel = $derived(`Observed · 100 % = ${reference.text}`)
  /**
   * The legend: the series that show, in a fixed order; beside the top label when both fit, else on
   * lines of their own under it (a narrow view), which move the plot down.
   */
  const legend = $derived.by(() => {
    const items: { color: string; text: string }[] = [
      { color: COLORS.b, text: 'b ions' },
      { color: COLORS.y, text: 'y ions' },
    ]
    if (groups.some((g) => g.type !== 'b' && g.type !== 'y') || bottomIons.some((b) => b.type !== 'b' && b.type !== 'y')) {
      items.push({ color: COLORS.other, text: 'other ions' })
    }
    if (groups.length < mz.length) items.push({ color: COLORS.unmatched, text: 'unmatched' })
    // each item: the swatch (10 px), 5 px, the text, 7 px to the next
    const widths = items.map((item) => textWidth(item.text, 11) + 22)
    const placed = placeLegend(textWidth(topLabel, 11), widths, M.left, width - M.right)
    return { items: items.map((item, i) => ({ ...item, ...placed.items[i] })), lines: placed.lines }
  })
  /** The strip above the plot (what the upper half is, the legend) and the one below the axis labels. */
  const plotTop = $derived(24 + legend.lines * line)
  const plotBottom = $derived(plotTop + plotHeight - 66)
  const mid = $derived((plotTop + plotBottom) / 2)
  const font = "'Inter Variable', ui-sans-serif, system-ui, sans-serif"
  // the theme's colours (app.css); the SVG export resolves them in the light theme
  const ink = 'var(--plot-ink)'
  const muted = 'var(--plot-muted)'
  const halo = 'var(--plot-halo)'

  /** The lower half's label, broken into lines between its parts where the view is too narrow for it. */
  const bottomLines = $derived(wrapParts(bottom.label.split(' · '), width - M.left - M.right))
  /** The similarity moves to a line of its own when the label and it would meet (a narrow view). */
  const stacked = $derived(bottomLines.length > 1 || !stripFits(bottom.label, similarity, width - M.left - M.right))
  /** The lines the label and the similarity take below their first. */
  const bottomExtra = $derived((bottomLines.length - 1 + (similarity && stacked ? 1 : 0)) * line)
  /**
   * The lower half's label (and the similarity, on its own line when narrow): with the error strip right under the
   * lower half, else under the m/z labels.
   */
  const bottomLabelY = $derived(errors ? plotBottom + 16 : plotBottom + 36 + line)
  /** The error strip: its header (one line or more), then its plot (errorTop to errorBottom). */
  const errorTop = $derived.by(() => bottomLabelY + bottomExtra + 30 + (errorLines.length - 1) * line)
  const errorBottom = $derived(errorTop + 84)
  /** The m/z labels sit below the lowest part: the lower half, or the error strip; the axis title a line under them. */
  const axisBase = $derived(errors ? errorBottom : plotBottom)
  const H = $derived(errors ? axisBase + 24 + line : axisBase + 42 + line + bottomExtra)
  let svg = $state<SVGSVGElement | null>(null)

  /** The SVG (for "Export spectrum (SVG)"). */
  export function svgElement(): SVGSVGElement | null {
    return svg
  }

  const bottomIons = $derived(bottom.kind === 'ions' ? bottom.ions : [])
  /** The precursor's m/z, when it is known. */
  const precursorMz = $derived(precursor && Number.isFinite(precursor.mz) && precursor.mz > 0 ? precursor.mz : null)
  const fullDomain = $derived(mzDomain([mz, bottomIons.map((b) => b.mz), precursorMz === null ? [] : [precursorMz]]))
  // zoom (survey P0-6): drag across the plot (with no tool on) to zoom into an m/z range; a double click (not on a
  // label of the user's) or "Reset zoom" goes back. The y scale stays that of the whole spectrum.
  let zoom = $state<[number, number] | null>(null)
  let brush = $state<{ from: number; to: number } | null>(null)
  /** A drag just zoomed: the click that ends it is not the tools'. */
  let dragged = false
  const domain = $derived(zoom ?? fullDomain)
  const inView = (v: number) => v >= domain[0] && v <= domain[1]
  /** The right end of the m/z axis. */
  const xEnd = $derived(Math.max(M.left + 10, width - M.right))
  const x = $derived(scale(domain, [M.left, xEnd]))
  const ref = $derived(reference.intensity > 0 ? reference.intensity : 1)
  /** Upper half: the ratio to the reference (0–1) to y, with room for the labels of the tallest peaks. */
  const upper = $derived((r: number) => mid - r * (mid - plotTop - 18))
  /** Lower half: 0–1 to y. */
  const lower = $derived((r: number) => mid + r * (plotBottom - mid - 6))

  const groupOf = $derived(new Map(groups.map((g) => [g.peak, g])))

  function colorOf(type: string): string {
    return type === 'b' ? COLORS.b : type === 'y' ? COLORS.y : COLORS.other
  }

  /** The observed sticks: unmatched light grey, matched by colour; above the reference clipped at 100 %. */
  const sticks = $derived.by(() => {
    const items: { mz: number; ratio: number; color: string }[] = []
    for (let i = 0; i < mz.length; i++) {
      const g = groupOf.get(i)
      items.push({ mz: mz[i], ratio: intensity[i] / ref, color: g ? colorOf(g.type) : COLORS.unmatched })
    }
    return stickGroups(items, x, upper)
  })

  /** The lower half's sticks, faint ones (theoretical ions without a match) apart. */
  const bottomSticks = $derived(
    stickGroups(
      bottomIons.map((b) => ({ mz: b.mz, ratio: b.height, color: colorOf(b.type), faint: b.faint })),
      x,
      lower,
    ),
  )

  /**
   * The precursor's mark: its x, its label ("precursor 2+") at the right of the triangle (at the left of it near the
   * right end) and the box the label and triangle take, which the ions' labels keep clear of.
   */
  const precursorMark = $derived.by(() => {
    // (zoomed to a range without it: not marked)
    if (precursorMz === null || !precursor || !inView(precursorMz)) return null
    const at = x(precursorMz)
    const text = precursor.charge > 0 ? `precursor ${precursor.charge}+` : 'precursor'
    const w = textWidth(text, 10) * 1.1
    const right = at + 7 + w <= width - M.right
    const box = { x0: right ? at - 6 : at - 7 - w, x1: right ? at + 7 + w : at + 6, y0: plotTop - 3, y1: plotTop + 12 }
    return { x: at, text, right, box }
  })
  const precursorDescription = $derived(
    precursorMz === null || !precursor
      ? ''
      : ` A dashed line marks the precursor at m/z ${precursorMz.toFixed(4)}${precursor.charge > 0 ? ` (${precursor.charge}+)` : ''}.`,
  )

  /** Where a peak's stick ends (clipped at 100 %). */
  const topOf = (p: PeakPoint) => upper(Math.min(1, p.intensity / ref))
  /** The baseline of a label right above a peak (above its clip mark, when clipped). */
  const labelBase = (p: PeakPoint) => topOf(p) - (p.intensity / ref > 1 ? 9 : 4)

  /**
   * The labels: the user's first (each where it fits, else right above its peak all the same), then those of the
   * matched peaks, the most intense first, clear of the user's, those without room left out.
   */
  const placedLabels = $derived.by(() => {
    const order = groups.filter((g) => inView(g.mz)).sort((p, q) => q.intensity - p.intensity)
    /** A box around a label (its baseline `y`, `height` above it). */
    const boxOf = (l: { x: number; y: number; width: number }, height: number): Box => ({ x0: l.x - l.width / 2, x1: l.x + l.width / 2, y0: l.y - height, y1: l.y })
    const own = marks.labels.map((l) => ({ x: x(l.mz), y: labelBase(l), width: textWidth(l.text, 11) * 1.1 + 12 }))
    const ownYs = placeLabels(own, { lineHeight: 15, tries: 4, top: plotTop + 10 })
    const user = marks.labels.map((l, i) => ({ ...own[i], y: ownYs[i] ?? Math.max(plotTop + 10, own[i].y), top: topOf(l), text: l.text, mz: l.mz }))
    const userBoxes = user.map((l) => boxOf(l, 15))
    const ys = placeLabels(
      order.map((g) => ({ x: x(g.mz), y: labelBase(g), width: textWidth(g.label, 11) + 8 })),
      { lineHeight: 12, tries: 4, top: plotTop + 10, taken: [...userBoxes, ...(precursorMark ? [precursorMark.box] : [])] },
    )
    const out: { x: number; y: number; text: string; ions: string; key: number; mz: number; width: number }[] = []
    order.forEach((g, i) => {
      const y = ys[i]
      if (y !== null) {
        out.push({ x: x(g.mz), y, text: g.label, ions: [...new Set(g.matches.map((m) => m.type))].join(' '), key: g.peak, mz: g.mz, width: textWidth(g.label, 11) + 8 })
      }
    })
    return { user, ions: out, boxes: [...userBoxes, ...out.map((l) => boxOf(l, 12))] }
  })
  const labels = $derived(placedLabels.ions)
  const userLabels = $derived(placedLabels.user)

  /** The measurements' brackets with their labels ("Δ163.063 ≈ Y"), above the higher of their peaks and the labels. */
  const measurementTexts = $derived(marks.measurements.map((m) => measurementText(m, measure)))
  const brackets = $derived(
    layoutBrackets(
      marks.measurements.map((m, i) => ({ xa: x(m.a.mz), ya: topOf(m.a), xb: x(m.b.mz), yb: topOf(m.b), width: textWidth(measurementTexts[i], 11) * 1.1 + 8 })),
      { top: plotTop + 14, labels: placedLabels.boxes },
    ),
  )

  const xTicks = $derived(niceTicks(domain[0], domain[1], Math.max(2, Math.floor((width - M.left - M.right) / 90))))

  // the error strip: the errors in the tolerance's unit, the band ±tolerance, a little room around it
  const unit = $derived(errors?.tolerance.unit ?? 'ppm')
  const errorOf = (m: PeakMatch) => fragmentError(m, mz, unit)
  const errorMax = $derived.by(() => {
    if (!errors) return 1
    const worst = Math.max(0, ...errors.matches.map((m) => Math.abs(errorOf(m))))
    return Math.max(errors.tolerance.value, worst) * 1.12 || 1
  })
  const ey = $derived(scale([errorMax, -errorMax], [errorTop, errorBottom]))
  const maxI = $derived.by(() => {
    let max = 0
    for (let i = 0; i < intensity.length; i++) if (intensity[i] > max) max = intensity[i]
    return max > 0 ? max : 1
  })
  // the large dots first, so that small ones stay visible on top
  const dots = $derived(
    errors
      ? [...errors.matches]
          .sort((p, q) => (intensity[q.peak] ?? 0) - (intensity[p.peak] ?? 0))
          .map((m) => ({ m, error: errorOf(m), r: 2.25 + 3.75 * Math.sqrt(Math.max(0, intensity[m.peak] ?? 0) / maxI) }))
      : [],
  )
  const stats = $derived(errors ? errorStats(errors.matches, errorOf) : { mean: NaN, sd: NaN, n: 0 })
  /** An error with its unit: ppm with one decimal, Da with three. */
  const errorText = (v: number) => `${signedNumber(v, unit === 'Da' ? 3 : 1)} ${unit}`
  /** Enough decimals that the ticks ±tolerance read as the tolerance (0.005 Da: three). */
  const tickDigits = $derived(errors ? errorTickDigits(errors.tolerance) : 1)
  const errorTick = (v: number) => signedNumber(v, tickDigits, { plus: false, trim: true })
  const errorHeader = $derived(
    !errors
      ? ''
      : errors.matches.length === 0
        ? 'Fragment mass error · no annotated ion shown'
        : `Fragment mass error of the annotated ions · mean ${errorText(stats.mean)}${Number.isFinite(stats.sd) ? ` · SD ${errorText(stats.sd).replace(/^[+−-]/, '')}` : ''} · dot size = intensity`,
  )
  const toleranceText = $derived(errors ? `tolerance ±${errors.tolerance.value} ${unit}` : '')
  /**
   * The header over the error strip: the tolerance at the right of the line when both fit, else the
   * header and the tolerance as one text broken into lines (a narrow view).
   */
  const errorLines = $derived(
    !errors || stripFits(errorHeader, toleranceText, width - M.left - M.right)
      ? [errorHeader]
      : wrapParts([...errorHeader.split(' · '), toleranceText], width - M.left - M.right),
  )
  /** The strip for screen readers, in sentences. */
  const errorDescription = $derived(
    !errors
      ? ''
      : errors.matches.length === 0
        ? ' Below it, no annotated ion is shown, so no mass error is shown.'
        : ` Below it, the mass error in ${unit} of ${errors.matches.length === 1 ? 'the annotated ion shown' : `each of the ${errors.matches.length} annotated ions shown`} is drawn under its peak, against its theoretical m/z, within the tolerance of ±${errors.tolerance.value} ${unit}, in the colours of the spectrum.` +
          ` The mean error is ${errorText(stats.mean)}${Number.isFinite(stats.sd) ? `, the standard deviation ${errorText(stats.sd).replace(/^[+−-]/, '')}` : ''}.`,
  )

  /** The measurements and labels for screen readers. */
  const marksDescription = $derived.by(() => {
    const parts: string[] = []
    if (marks.measurements.length > 0) {
      const each = marks.measurements.map(
        (m, i) => `${measurementTexts[i].replace(/^Δ/, 'Δm/z ')} between m/z ${Math.min(m.a.mz, m.b.mz).toFixed(3)} and ${Math.max(m.a.mz, m.b.mz).toFixed(3)}`,
      )
      parts.push(` Measured: ${each.join('; ')}.`)
    }
    if (marks.labels.length > 0) parts.push(` Your labels: ${marks.labels.map((l) => `“${l.text}” at m/z ${l.mz.toFixed(4)}`).join('; ')}.`)
    return parts.join('')
  })

  // the tools: the peak a measurement starts at, the peak the pointer would snap to, what is selected, the text field
  let start = $state<number | null>(null)
  let snap = $state<number | null>(null)
  let selected = $state<{ kind: 'measurement' | 'label'; index: number } | null>(null)
  let editor = $state<{ mz: number; intensity: number; text: string } | null>(null)
  /** What the tools did, for screen readers. */
  let status = $state('')
  let container = $state<HTMLDivElement | null>(null)
  /** The selection, while what it selects is there. */
  const selection = $derived(
    selected && selected.index < (selected.kind === 'measurement' ? marks.measurements.length : marks.labels.length) ? selected : null,
  )
  // (another spectrum or tool: what was half made goes; another spectrum: the selection too)
  $effect.pre(() => {
    void [mz, tool]
    untrack(() => {
      start = null
      snap = null
      editor = null
    })
  })
  $effect.pre(() => {
    void mz
    untrack(() => {
      selected = null
      zoom = null
      brush = null
    })
  })
  $effect(() => {
    pending = start !== null || editor !== null
  })

  const pointOf = (i: number): PeakPoint => ({ mz: mz[i], intensity: intensity[i] })
  const mzAt = (at: number) => domain[0] + ((at - M.left) / (xEnd - M.left)) * (domain[1] - domain[0])
  /** The observed peak a pointer at (`at`, `py`) snaps to, or -1: above the axis only. */
  function snapAt(at: number, py: number): number {
    if (py < plotTop - 4 || py > mid + 4 || at < M.left - 10 || at > xEnd + 10) return -1
    return snapPeak(mz, x, mzAt, (i) => topOf(pointOf(i)), at, py)
  }
  function pointer(event: MouseEvent): { at: number; py: number } {
    const box = svg?.getBoundingClientRect()
    return { at: event.clientX - (box?.left ?? 0), py: event.clientY - (box?.top ?? 0) }
  }
  /** The user label whose text is at (`at`, `py`), or -1. */
  function userLabelAt(at: number, py: number): number {
    return userLabels.findIndex((l) => inView(l.mz) && at >= l.x - l.width / 2 && at <= l.x + l.width / 2 && py >= l.y - 13 && py <= l.y + 3)
  }

  /** Whether (`at`, `py`) is at the top of the peak `i` (within 8 px on screen): a click there is the tool's. */
  function atTopOf(i: number, at: number, py: number): boolean {
    return i >= 0 && Math.hypot(x(mz[i]) - at, topOf(pointOf(i)) - py) <= 8
  }

  function onclick(event: MouseEvent) {
    if (dragged) return void (dragged = false)
    const { at, py } = pointer(event)
    // a label or a bracket first (not while a measurement is half made: its second peak may be under one; nor a
    // bracket at the top of a peak with a tool on: a tall peak may reach into it)
    if (start === null) {
      const l = userLabelAt(at, py)
      if (l >= 0) {
        const again = selection?.kind === 'label' && selection.index === l
        return select(
          again ? null : { kind: 'label', index: l },
          again ? 'Label deselected.' : `Label “${marks.labels[l].text}” selected; Delete removes it, a double click edits it.`,
        )
      }
      const b = tool && atTopOf(snapAt(at, py), at, py) ? -1 : bracketAt(brackets, at, py)
      if (b >= 0) {
        const again = selection?.kind === 'measurement' && selection.index === b
        return select(again ? null : { kind: 'measurement', index: b }, again ? 'Measurement deselected.' : `Measurement ${measurementTexts[b]} selected; Delete removes it.`)
      }
    }
    selected = null
    if (tool === 'measure') {
      const i = snapAt(at, py)
      if (i < 0) return
      container?.focus({ preventScroll: true })
      if (start === null) {
        start = i
        status = `Measuring from m/z ${mz[i].toFixed(3)}: click the second peak (Esc cancels).`
      } else if (i !== start) {
        const m = { a: pointOf(start), b: pointOf(i) }
        start = null
        onmarks({ ...marks, measurements: [...marks.measurements, m] })
        status = `Measured ${measurementText(m, measure)}.`
      }
    } else if (tool === 'label') {
      const i = snapAt(at, py)
      if (i >= 0) openEditor(pointOf(i))
    }
  }

  function select(next: typeof selected, said: string) {
    selected = next
    status = said
    container?.focus({ preventScroll: true })
  }

  function ondblclick(event: MouseEvent) {
    const { at, py } = pointer(event)
    const l = userLabelAt(at, py)
    if (l >= 0) openEditor(marks.labels[l])
    else zoom = null
  }

  // the zoom's brush: only with no tool on (with one, the clicks are the tool's)
  function onpointerdown(event: PointerEvent) {
    dragged = false
    const { at, py } = pointer(event)
    if (tool || event.button !== 0 || at < M.left || at > xEnd || py < plotTop || py > plotBottom) return
    brush = { from: at, to: at }
    svg?.setPointerCapture(event.pointerId)
  }

  function onpointerup() {
    if (brush && Math.abs(brush.to - brush.from) > 5) {
      zoom = [mzAt(Math.min(brush.from, brush.to)), mzAt(Math.max(brush.from, brush.to))]
      dragged = true
    }
    brush = null
  }

  /** The text field at a peak: its label's text, if it has one. */
  function openEditor(p: PeakPoint) {
    const at = labelIndexAt(marks.labels, p.mz)
    const peak = at >= 0 ? marks.labels[at] : p
    editor = { mz: peak.mz, intensity: peak.intensity, text: at >= 0 ? marks.labels[at].text : '' }
    selected = null
  }
  function commitEditor() {
    const e = editor
    if (!e) return
    editor = null
    onmarks({ ...marks, labels: withLabel(marks.labels, e, e.text) })
    status = e.text.trim() === '' ? `Label at m/z ${e.mz.toFixed(4)} removed.` : `Label “${e.text.trim()}” at m/z ${e.mz.toFixed(4)}.`
    container?.focus({ preventScroll: true })
  }
  function oneditorkeydown(event: KeyboardEvent) {
    if (event.key === 'Enter') {
      event.preventDefault()
      commitEditor()
    } else if (event.key === 'Escape') {
      event.preventDefault()
      event.stopPropagation()
      editor = null
      container?.focus({ preventScroll: true })
    }
  }

  /** Cancels what is half done, else the selection; whether there was something (Esc: the caller's keys). */
  export function cancel(): boolean {
    if (editor) editor = null
    else if (start !== null) {
      start = null
      status = 'Measurement cancelled.'
    } else if (selection) selected = null
    else return false
    return true
  }

  /** Removes the selected measurement or label; whether one was (Delete, Backspace: the caller's keys). */
  export function deleteSelection(): boolean {
    const s = selection
    if (!s) return false
    selected = null
    if (s.kind === 'measurement') {
      status = `Measurement ${measurementTexts[s.index]} removed.`
      onmarks({ ...marks, measurements: marks.measurements.filter((_, i) => i !== s.index) })
    } else {
      status = `Label “${marks.labels[s.index].text}” removed.`
      onmarks({ ...marks, labels: marks.labels.filter((_, i) => i !== s.index) })
    }
    return true
  }

  // hover: the peak (or predicted ion) under the pointer, the most intense within 6 px
  let tip = $state<{ x: number; y: number; lines: string[] } | null>(null)

  function strongestNear(values: ArrayLike<number>, heights: ArrayLike<number>, at: number): number {
    const i = nearest(values, at)
    if (i < 0) return -1
    let best = -1
    for (let j = i; j >= 0 && Math.abs(x(values[j]) - x(at)) <= 6; j--) if (best < 0 || heights[j] > heights[best]) best = j
    for (let j = i + 1; j < values.length && Math.abs(x(values[j]) - x(at)) <= 6; j++) if (best < 0 || heights[j] > heights[best]) best = j
    return best
  }

  function onpointermove(event: PointerEvent) {
    if (!svg) return
    const box = svg.getBoundingClientRect()
    const px0 = event.clientX - box.left
    const py = event.clientY - box.top
    if (brush) {
      brush.to = Math.min(xEnd, Math.max(M.left, px0))
      tip = null
      return
    }
    if (tool) {
      const i = snapAt(px0, py)
      snap = i < 0 ? null : i
      // (a measurement half made: the live Δm/z says it, no tooltip over the brackets)
      if (start !== null) return void ((tip = null), (hover = null))
    }
    if (px0 < M.left - 4 || px0 > width - M.right + 4 || py < plotTop - 4 || py > plotBottom) return void ((tip = null), (hover = null))
    const at = mzAt(px0)
    if (precursorMark && precursor && py <= plotTop + 12 && px0 >= precursorMark.box.x0 && px0 <= precursorMark.box.x1) {
      hover = null
      tip = { x: precursorMark.x, y: py, lines: [`Precursor · m/z ${precursor.mz.toFixed(4)}${precursor.charge > 0 ? ` · ${precursor.charge}+` : ''}`] }
    } else if (py <= mid) {
      const i = strongestNear(mz, intensity, at)
      if (i < 0) return void (tip = null)
      const g = groupOf.get(i)
      const share = (intensity[i] / ref) * 100
      const lines = [`m/z ${mz[i].toFixed(4)} · ${share.toFixed(1)} %${share > 100 ? ' (clipped)' : ''}`]
      // (the error in the search's unit, as the strip below gives it)
      hover = g ? mz[i] : null
      if (g) for (const m of g.matches) lines.push(`${ionLabel(m.ion)} · ${errors ? errorText(errorOf(m)) : `${signedNumber(m.errorPpm, 2)} ppm`}`)
      else lines.push('unmatched')
      tip = { x: x(mz[i]), y: py, lines }
    } else if (bottom.kind === 'ions' && bottomIons.length > 0) {
      const i = strongestNear(
        bottomIons.map((b) => b.mz),
        bottomIons.map((b) => b.height),
        at,
      )
      if (i < 0) return void (tip = null)
      const b = bottomIons[i]
      const what = bottom.relative ? ` · ${(b.height * 100).toFixed(1)} %` : b.faint ? ' · not matched' : ' · matched'
      tip = { x: x(b.mz), y: py, lines: [`${ionLabel(b.ion)} · m/z ${b.mz.toFixed(4)}${what}`] }
    } else {
      tip = null
    }
  }
</script>

<div class="relative outline-none" bind:clientWidth={width} bind:this={container} tabindex="-1" data-testid="spectrum-area">
  <!-- (the clicks are the pointer's tools; what they make is listed in the description, and the toolbar's buttons and
       the keys clear and remove it) -->
  <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_noninteractive_element_interactions -->
  <svg
    bind:this={svg}
    data-testid="spectrum"
    role="img"
    aria-labelledby="{uid}-title"
    aria-describedby="{uid}-desc"
    {width}
    height={H}
    viewBox="0 0 {width} {H}"
    font-family={font}
    class="block"
    style:cursor="crosshair"
    style:touch-action="none"
    data-tool={tool ?? undefined}
    data-mz-domain="{domain[0]} {domain[1]}"
    data-x-range="{M.left} {xEnd}"
    {onpointermove}
    {onclick}
    {ondblclick}
    {onpointerdown}
    {onpointerup}
    onpointercancel={() => (brush = null)}
    onlostpointercapture={() => (brush = null)}
    onpointerleave={() => {
      tip = null
      snap = null
      hover = null
    }}>
    <defs>
      <clipPath id="{uid}-clip"><rect x={M.left} y="0" width={Math.max(0, xEnd - M.left)} height={H} /></clipPath>
    </defs>
    <title id="{uid}-title">{title}</title>
    <desc id="{uid}-desc">{description}{precursorDescription}{errorDescription}{marksDescription}</desc>

    <!-- grid: hairlines at the m/z ticks, in the spectrum and in the error strip -->
    {#each xTicks as t (t)}
      <line x1={px(x(t))} x2={px(x(t))} y1={plotTop} y2={plotBottom} stroke="var(--plot-band)" stroke-width="1" />
      {#if errors}<line x1={px(x(t))} x2={px(x(t))} y1={errorTop} y2={errorBottom} stroke="var(--plot-band)" stroke-width="1" />{/if}
    {/each}

    <!-- the precursor: a dashed line at its m/z through the upper half, a triangle and its label at the top (above the
         peaks, which stop below them) -->
    {#if precursorMark}
      <g data-testid="precursor-mark" data-mz={precursorMz}>
        <line x1={px(precursorMark.x)} x2={px(precursorMark.x)} y1={plotTop + 9} y2={mid} stroke={muted} stroke-width="1" stroke-dasharray="3 3" />
        <path d="M{px(precursorMark.x - 4.5)} {plotTop + 2}h9l-4.5 6z" fill={ink} />
        <text
          x={px(precursorMark.right ? precursorMark.x + 7 : precursorMark.x - 7)}
          y={plotTop + 9}
          text-anchor={precursorMark.right ? 'start' : 'end'}
          font-size="10"
          fill={ink}
          data-testid="precursor-label">{precursorMark.text}</text>
      </g>
    {/if}

    <!-- (zoomed: what lies outside the m/z range is clipped) -->
    <g clip-path="url(#{uid}-clip)">
    <!-- the observed peaks: unmatched light grey and thin, matched in their series' colour -->
    {#each sticks as s (s.color)}
      {@const matched = s.color !== COLORS.unmatched}
      <path
        d={stickPath(s.xs, s.tops, mid)}
        stroke={s.color}
        stroke-width={matched ? 1.75 : 1}
        stroke-linecap={matched ? 'round' : 'butt'}
        fill="none"
        data-testid={matched ? 'matched-peaks' : 'unmatched-peaks'} />
      {#if s.clipped.length > 0}
        <path d={clipMarks(s.clipped, upper(1) - 1, true)} fill={s.color} data-testid="clipped-peaks" />
      {/if}
    {/each}

    <!-- the lower half -->
    {#each bottomSticks as s (`${s.color}/${s.faint}`)}
      <path
        d={stickPath(s.xs, s.tops, mid)}
        stroke={s.color}
        stroke-width={s.faint ? 1 : bottom.kind === 'ions' && !bottom.relative ? 1.25 : 1.75}
        stroke-opacity={s.faint ? 0.4 : 1}
        stroke-linecap="round"
        fill="none"
        data-testid="bottom-peaks" />
    {/each}
    {#if bottom.kind === 'status'}
      <text
        x={px((M.left + width - M.right) / 2)}
        y={px(mid + (plotBottom - mid) / 2 - (bottom.failed && onTheoretical ? 12 : 0))}
        text-anchor="middle"
        font-size="12"
        fill={muted}
        data-testid="bottom-status"
        data-no-export="">{bottom.message}</text>
    {/if}
    </g>

    <!-- the fragment mass errors, under their peaks -->
    {#if errors}
      <g data-testid="mass-error">
        <text x={M.left} y={errorTop - 10} font-size="11" fill={muted} data-testid="error-stats"
          >{#each errorLines as text, i (i)}<tspan x={M.left} y={errorTop - 10 - (errorLines.length - 1 - i) * line}>{text}</tspan>{/each}</text>
        {#if errorLines.length === 1}
          <text x={width - M.right} y={errorTop - 10} text-anchor="end" font-size="10" fill={muted}>{toleranceText}</text>
        {/if}
        <rect
          x={M.left}
          y={px(ey(errors.tolerance.value))}
          width={Math.max(0, width - M.right - M.left)}
          height={Math.max(0, ey(-errors.tolerance.value) - ey(errors.tolerance.value))}
          fill="var(--plot-band)"
          fill-opacity="0.8"
          data-testid="tolerance-band" />
        {#each [errors.tolerance.value, 0, -errors.tolerance.value] as t (t)}
          <line x1={M.left} x2={width - M.right} y1={px(ey(t))} y2={px(ey(t))} stroke={t === 0 ? 'var(--plot-zero)' : 'var(--plot-rule)'} stroke-width="1" />
          <text x={M.left - 6} y={px(ey(t) + 3.5)} text-anchor="end" font-size="10" fill={muted}>{errorTick(t)}</text>
        {/each}
        <text x={M.left - 6} y={errorTop - 10} text-anchor="end" font-size="10" fill={muted} data-testid="error-unit">{unit}</text>
        <g clip-path="url(#{uid}-clip)">
        {#each dots as d, i (i)}
          <circle
            cx={px(x(mz[d.m.peak]))}
            cy={px(ey(d.error))}
            r={px(d.r)}
            fill={ionColor(d.m.type)}
            stroke={halo}
            stroke-width="1.25"
            data-testid="error-dot"
            data-type={d.m.type}
            data-ion={d.m.ion}
            data-error={d.error}
            data-mz={mz[d.m.peak]}
            ><title>{ionLabel(d.m.ion)} · m/z {mz[d.m.peak].toFixed(4)} · {errorText(d.error)}</title></circle>
        {/each}
        </g>
      </g>
    {/if}

    <!-- the m/z highlighted with the ion table, and the zoom brush (page only) -->
    {#if hover !== null && inView(hover)}
      <line x1={px(x(hover))} x2={px(x(hover))} y1={plotTop} y2={axisBase} stroke="var(--color-ring)" stroke-width="2" stroke-dasharray="3 2" data-no-export="" data-testid="hover-line" />
    {/if}
    {#if brush}
      <rect x={Math.min(brush.from, brush.to)} y={plotTop} width={Math.abs(brush.to - brush.from)} height={plotBottom - plotTop} fill="var(--color-ring)" fill-opacity="0.15" data-no-export="" />
    {/if}

    <!-- the axis: the halves' baseline, the m/z labels below everything -->
    <line x1={M.left} x2={width - M.right} y1={mid} y2={mid} stroke={muted} stroke-width="1" />
    {#each xTicks as t (t)}
      {#if x(t) < width - M.right - 30}<text x={px(x(t))} y={axisBase + 15} text-anchor="middle" font-size="11" fill={muted}>{t}</text>{/if}
    {/each}
    <!-- (the axis title centred under the tick labels) -->
    <text x={px((M.left + width - M.right) / 2)} y={axisBase + 15 + line} text-anchor="middle" font-size="11" fill={muted} data-testid="mz-axis-title">m/z</text>

    <!-- relative intensity, of each half's 100 % -->
    <g font-size="10" fill={muted} text-anchor="end">
      {#each [1, 0.5] as r (r)}
        <text x={M.left - 6} y={px(upper(r) + 3.5)}>{r * 100} %</text>
        <line x1={M.left - 3} x2={M.left} y1={px(upper(r))} y2={px(upper(r))} stroke={muted} stroke-width="1" />
        {#if bottom.kind === 'ions' && bottom.relative}
          <text x={M.left - 6} y={px(lower(r) + 3.5)}>{r * 100} %</text>
          <line x1={M.left - 3} x2={M.left} y1={px(lower(r))} y2={px(lower(r))} stroke={muted} stroke-width="1" />
        {/if}
      {/each}
    </g>

    <g clip-path="url(#{uid}-clip)">
    <!-- direct labels of the matched peaks -->
    {#each labels as l (l.key)}
      <text
        x={px(l.x)}
        y={px(l.y)}
        text-anchor="middle"
        font-size="11"
        font-weight="600"
        fill={ink}
        stroke={halo}
        stroke-width="3"
        stroke-linejoin="round"
        paint-order="stroke"
        data-testid="peak-label"
        data-ions={l.ions}
        data-mz={l.mz}>{l.text}</text>
    {/each}

    <!-- the user's measurements: a bracket above the higher peak, dotted lines down to the peaks, the Δm/z -->
    {#each brackets as b, i (i)}
      {@const isSelected = selection?.kind === 'measurement' && selection.index === i}
      <g data-testid="measurement" data-text={measurementTexts[i]} data-selected={isSelected ? '' : undefined}>
        <line x1={px(b.xa)} x2={px(b.xb)} y1={px(b.y)} y2={px(b.y)} stroke="var(--plot-measure)" stroke-width="1.75" />
        {#if b.da - b.y > 1}<line x1={px(b.xa)} x2={px(b.xa)} y1={px(b.da)} y2={px(b.y)} stroke="var(--plot-measure)" stroke-width="1" stroke-dasharray="2 2" />{/if}
        {#if b.db - b.y > 1}<line x1={px(b.xb)} x2={px(b.xb)} y1={px(b.db)} y2={px(b.y)} stroke="var(--plot-measure)" stroke-width="1" stroke-dasharray="2 2" />{/if}
        <text
          x={px((b.xa + b.xb) / 2)}
          y={px(b.y - 4)}
          text-anchor="middle"
          font-size="11"
          font-weight="600"
          fill="var(--plot-measure)"
          stroke={halo}
          stroke-width="3"
          stroke-linejoin="round"
          paint-order="stroke">{measurementTexts[i]}</text>
        {#if isSelected}
          <!-- (the selection: the page's only, over the bracket in the colour of a selected one) -->
          <g data-no-export="" data-testid="measurement-selection">
            <rect
              x={px(b.box.x0 - 3)}
              y={px(b.box.y0 - 2)}
              width={px(b.box.x1 - b.box.x0 + 6)}
              height={px(b.y - b.box.y0 + 6)}
              rx="3"
              fill="var(--plot-measure-selected)"
              fill-opacity="0.12"
              stroke="var(--plot-measure-selected)"
              stroke-width="1" />
            <line x1={px(b.xa)} x2={px(b.xb)} y1={px(b.y)} y2={px(b.y)} stroke="var(--plot-measure-selected)" stroke-width="1.75" />
            <text
              x={px((b.xa + b.xb) / 2)}
              y={px(b.y - 4)}
              text-anchor="middle"
              font-size="11"
              font-weight="600"
              fill="var(--plot-measure-selected)">{measurementTexts[i]}</text>
          </g>
        {/if}
      </g>
    {/each}

    <!-- the user's labels, above their peaks (a leader line down to the peak when the label had to go up) -->
    {#each userLabels as l, i (i)}
      {@const isSelected = selection?.kind === 'label' && selection.index === i}
      <g data-testid="user-label" data-mz={l.mz} data-selected={isSelected ? '' : undefined}>
        {#if isSelected}
          <rect
            x={px(l.x - l.width / 2)}
            y={px(l.y - 13)}
            width={px(l.width)}
            height="17"
            rx="3"
            fill="var(--plot-user-label)"
            fill-opacity="0.12"
            stroke="var(--plot-user-label)"
            stroke-width="1"
            data-no-export="" />
        {/if}
        {#if l.top - l.y > 8}
          <line x1={px(l.x)} x2={px(l.x)} y1={px(l.y + 3)} y2={px(l.top - 2)} stroke="var(--plot-user-label)" stroke-width="1" stroke-dasharray="1 2" />
        {/if}
        <text
          x={px(l.x)}
          y={px(l.y)}
          text-anchor="middle"
          font-size="11"
          font-weight="600"
          fill="var(--plot-user-label)"
          stroke={halo}
          stroke-width="3"
          stroke-linejoin="round"
          paint-order="stroke">{l.text}</text>
      </g>
    {/each}

    </g>

    <!-- the tools under the pointer (the page's only): the peak it would snap to, the start of a measurement, the
         live Δm/z to the peak under the pointer -->
    {#if tool && snap !== null && snap < mz.length}
      <circle
        cx={px(x(mz[snap]))}
        cy={px(topOf(pointOf(snap)))}
        r="4.5"
        fill="none"
        stroke={tool === 'measure' ? 'var(--plot-measure)' : 'var(--plot-user-label)'}
        stroke-width="1.5"
        data-testid="snap-ring"
        data-no-export="" />
    {/if}
    {#if start !== null && start < mz.length}
      {@const sx = x(mz[start])}
      {@const sy = topOf(pointOf(start))}
      <circle cx={px(sx)} cy={px(sy)} r="4" fill="var(--plot-measure)" stroke={halo} stroke-width="1" data-testid="measure-start" data-no-export="" />
      {#if snap !== null && snap !== start && snap < mz.length}
        {@const hx = x(mz[snap])}
        {@const hy = topOf(pointOf(snap))}
        <g data-testid="measure-preview" data-no-export="">
          <line x1={px(sx)} y1={px(sy)} x2={px(hx)} y2={px(hy)} stroke="var(--plot-measure)" stroke-width="1.25" stroke-dasharray="3 3" />
          <text
            x={px((sx + hx) / 2)}
            y={px(Math.max(plotTop + 10, Math.min(sy, hy) - 8))}
            text-anchor="middle"
            font-size="11"
            font-weight="600"
            fill="var(--plot-measure)"
            stroke={halo}
            stroke-width="3"
            stroke-linejoin="round"
            paint-order="stroke">{deltaText(Math.abs(mz[snap] - mz[start]))}</text>
        </g>
      {/if}
    {/if}


    <!-- above: what the upper half is and the legend; below: what the lower half is, the similarity -->
    <text x={M.left} y="14" font-size="11" fill={muted} data-testid="top-label">{topLabel}</text>
    {#each legend.items as item (item.text)}
      <g data-testid="legend-item" data-line={item.line}>
        <rect x={px(item.x)} y={5 + item.line * line} width="10" height="10" rx="2" fill={item.color} />
        <text x={px(item.x + 15)} y={14 + item.line * line} font-size="11" fill={ink}>{item.text}</text>
      </g>
    {/each}
    <text
      x={M.left}
      y={bottomLabelY}
      font-size="11"
      fill={muted}
      data-testid="bottom-label"
      data-no-export={bottom.kind === 'status' ? '' : undefined}
      >{#each bottomLines as text, i (i)}<tspan x={M.left} y={bottomLabelY + i * line}>{text}</tspan>{/each}</text>
    {#if similarity}
      <text
        x={stacked ? M.left : width - M.right}
        y={bottomLabelY + (stacked ? bottomLines.length * line : 0)}
        text-anchor={stacked ? 'start' : 'end'}
        font-size="11"
        fill={muted}
        data-testid="similarity"
        data-stacked={stacked ? '' : undefined}>{similarity}</text>
    {/if}
  </svg>

  {#if zoom}
    <button
      type="button"
      class="absolute top-0 right-3 rounded-md border bg-background px-2 py-0.5 text-xs hover:bg-muted"
      data-testid="reset-zoom"
      onclick={() => (zoom = null)}>Reset zoom (m/z {zoom[0].toFixed(0)}–{zoom[1].toFixed(0)})</button>
  {/if}

  {#if bottom.kind === 'status' && bottom.failed && onTheoretical}
    <button
      type="button"
      class="absolute left-1/2 -translate-x-1/2 rounded-md border bg-background px-2.5 py-1 text-xs hover:bg-muted focus-visible:ring-2 focus-visible:ring-ring/50 focus-visible:outline-none"
      style:top="{mid + (plotBottom - mid) / 2}px"
      onclick={onTheoretical}>Show the theoretical spectrum</button>
  {/if}

  {#if editor}
    {@const ex = x(editor.mz)}
    {@const ey0 = topOf(editor)}
    <div
      class="absolute z-20 rounded-md border bg-background p-1 shadow-sm"
      style:left="{Math.min(Math.max(0, ex - 80), Math.max(0, width - 170))}px"
      style:top="{Math.max(0, ey0 - 40)}px"
      data-testid="label-editor">
      <input
        type="text"
        class="h-6 w-40 rounded-sm border bg-background px-1.5 text-xs outline-none focus-visible:ring-2 focus-visible:ring-ring/50"
        aria-label="Label for the peak at m/z {editor.mz.toFixed(4)} (Enter keeps it, Esc cancels; empty removes the label)"
        placeholder="Label for m/z {editor.mz.toFixed(2)}"
        value={editor.text}
        oninput={(e) => {
          if (editor) editor.text = e.currentTarget.value
        }}
        onkeydown={oneditorkeydown}
        onblur={commitEditor}
        {@attach (el) => el.focus()} />
    </div>
  {/if}

  <p class="sr-only" aria-live="polite" data-testid="tool-status">{status}</p>


  {#if tip}
    <div
      class="pointer-events-none absolute z-10 rounded-md border bg-background px-2 py-1 text-xs whitespace-nowrap shadow-sm tabular-nums"
      style:left="{tip.x > width - 200 ? tip.x - 10 : tip.x + 10}px"
      style:top="{tip.y}px"
      style:transform={tip.x > width - 200 ? 'translate(-100%, -50%)' : 'translateY(-50%)'}
      data-testid="peak-tooltip">
      {#each tip.lines as line, i (i)}<div class={i === 0 ? 'text-foreground' : 'text-muted-foreground'}>{line}</div>{/each}
    </div>
  {/if}
</div>
