// @vitest-environment happy-dom
/// <reference types="node" />
// The PSM view on the app's example (src/lib/results/fixtures/example: a search of the pinned ProSE with PeptDeep):
// the fragment mass errors of every annotated ion shown (any ion type, charge, loss, the precursor), in the colours of
// the spectrum, and the strip collapsed; the PSM table under the spectrum; the tools Measure (Δm/z, the residue it may
// be) and Label (texts of one's own on peaks), their keys, and what of them the SVG export has.
import { readFileSync } from 'node:fs'
import { basename, join } from 'node:path'
import { createRawSnippet, flushSync, mount, tick, unmount } from 'svelte'
import { afterAll, afterEach, beforeAll, describe, expect, it } from 'vitest'
import { input } from '../../design/fixtures'
import { resolveDesign } from '../../design/model'
import { bytesOfRange, FileNotFound, readSearch } from '../../results/read'
import { releaseResultsView, resultsViewFor, type ResultsView } from '../../results/view.svelte'
import type { OpenResult } from '../../search/session.svelte'
import { displaySettings } from '../../format/settings.svelte'
import { ionColor, spectrumPeaks } from '../../psm/psm'
import { exportSpectrumSvg } from '../../psm/svg-export'
import { textWidth } from '../../psm/plot'
import PsmView from './PsmView.svelte'

// from the root of the project: under happy-dom, import.meta.url is not a file URL
const fixture = join(process.cwd(), 'src/lib/results/fixtures/example')

async function open(parameters: Record<string, unknown> = {}): Promise<OpenResult> {
  const read = await readSearch(async (name, range) => {
    try {
      return bytesOfRange(new Uint8Array(readFileSync(join(fixture, name))), range)
    } catch (e) {
      if ((e as NodeJS.ErrnoException).code === 'ENOENT') throw new FileNotFound(name)
      throw e
    }
  })
  return {
    id: 'example',
    createdAt: '2026-11-01T00:00:00Z',
    design: resolveDesign(read.tables.files.map((f, i) => input(basename(f), { condition: 'control', replicate: i + 1 }))),
    psmFdr: 0.01,
    proteinFdr: 0.01,
    settings: { parameters, fixedMods: ['Carbamidomethyl (C)'], database: 'database.fasta', rescore: false, peptdeep: true, contaminants: false },
    outcome: { ...read, seconds: 1 },
  }
}

const rowOf = (view: ResultsView, sequence: string) => {
  const { psms, peptides } = view.model.tables
  const row = Array.from(psms.peptide).findIndex((k) => peptides.sequence[k] === sequence)
  expect(row, sequence).toBeGreaterThanOrEqual(0)
  return row
}

/** Waits for the spectrum of the PSM to be read. */
async function settle(target: HTMLElement) {
  for (let i = 0; i < 100 && target.querySelector('[data-testid="spectrum-loading"]'); i++) {
    await new Promise((r) => setTimeout(r, 10))
    await tick()
    flushSync()
  }
  expect(target.querySelector('[data-testid="spectrum-loading"]')).toBeNull()
}

// a PSM of the example with two peaks a Y apart, and one of another spectrum
const PSM = 'DLLTAYK'
const OTHER = 'TWFVEAK'
// a PSM with many annotated ions of every kind once all are shown
const RICH = 'VATEFSETAPATLK'

describe('the PSM view', () => {
  let shown: Record<string, unknown> | null = null
  afterEach(() => {
    if (shown) unmount(shown)
    shown = null
    releaseResultsView()
    document.body.innerHTML = ''
  })

  async function show(sequence: string, props: Record<string, unknown> = {}) {
    const view = resultsViewFor(await open())
    view.psm = { row: rowOf(view, sequence), origin: { kind: 'psms' } }
    const target = document.createElement('div')
    document.body.append(target)
    shown = mount(PsmView, { target, props: { view, onClose: () => {}, onBreadcrumb: () => {}, ...props } })
    flushSync()
    await settle(target)
    return { view, target, text: (id: string) => target.querySelector(`[data-testid="${id}"]`)?.textContent?.replace(/\s+/g, ' ').trim() ?? null }
  }

  it('gives the mass errors of every annotated ion shown: all ion types, charges and losses, in their colours', async () => {
    const { view, target, text } = await show(RICH)
    const dotsOf = () => Array.from(target.querySelectorAll('[data-testid="error-dot"]'))
    const plain = dotsOf()
    expect(plain.length).toBeGreaterThan(0)
    expect([...new Set(plain.map((d) => d.getAttribute('data-type')))].every((t) => t === 'b' || t === 'y')).toBe(true)
    // every ion type, 1+ and 2+, the losses and the precursor
    view.psmOptions.ions = ['a', 'b', 'c', 'x', 'y', 'z']
    view.psmOptions.charges = [1, 2]
    view.psmOptions.losses = ['H2O', 'NH3', 'other']
    view.psmOptions.precursor = true
    flushSync()
    const dots = dotsOf()
    expect(dots.length).toBeGreaterThan(plain.length)
    const types = new Set(dots.map((d) => d.getAttribute('data-type')))
    expect(types).toEqual(new Set(['a', 'b', 'y', 'z']))
    // each dot an ion of the spectrum's labels, in the colour of its series, within the search's 20 ppm
    for (const d of dots) {
      expect(d.getAttribute('fill'), d.getAttribute('data-ion') ?? '').toBe(ionColor(d.getAttribute('data-type') ?? ''))
      expect(Math.abs(Number(d.getAttribute('data-error'))), d.getAttribute('data-ion') ?? '').toBeLessThanOrEqual(20)
    }
    expect(dots.some((d) => /-H2O|-NH3/.test(d.getAttribute('data-ion') ?? ''))).toBe(true)
    expect(dots.some((d) => (d.getAttribute('data-ion') ?? '').endsWith('++'))).toBe(true)
    expect(text('error-stats')).toMatch(/^Fragment mass error of the annotated ions · ?mean [+−-]?\d+\.\d ppm/)
    expect(target.querySelector('[data-testid="spectrum"] desc')?.textContent).toMatch(
      new RegExp(`the mass error in ppm of each of the ${dots.length} annotated ions shown is drawn under its peak, against its theoretical m/z`),
    )
    // the toggles take their dots with them
    view.psmOptions.ions = ['y']
    view.psmOptions.precursor = false
    flushSync()
    expect(new Set(dotsOf().map((d) => d.getAttribute('data-type')))).toEqual(new Set(['y']))
    view.psmOptions.ions = []
    view.psmOptions.losses = []
    flushSync()
    expect(dotsOf()).toHaveLength(0)
    expect(text('error-stats')).toMatch(/^Fragment mass error · ?no annotated ion shown/)
  })

  it('collapses the mass errors under the spectrum, and keeps them collapsed for the next PSM', async () => {
    try {
      const { target } = await show(PSM)
      const toggle = target.querySelector<HTMLInputElement>('[data-testid="mass-errors-toggle"]')!
      expect(toggle.checked).toBe(true)
      expect(target.querySelectorAll('[data-testid="error-dot"]').length).toBeGreaterThan(0)
      toggle.click()
      flushSync()
      expect(displaySettings.massErrors).toBe(false)
      expect(target.querySelectorAll('[data-testid="error-dot"]').length).toBe(0)
      expect(target.querySelector('[data-testid="mass-error"]')).toBeNull()
      // the spectrum and its lower half stay
      expect(target.querySelector('[data-testid="bottom-label"]')).not.toBeNull()
      unmount(shown!)
      shown = null
      releaseResultsView()
      const again = await show(PSM)
      expect(again.target.querySelector<HTMLInputElement>('[data-testid="mass-errors-toggle"]')!.checked).toBe(false)
      expect(again.target.querySelectorAll('[data-testid="error-dot"]').length).toBe(0)
    } finally {
      displaySettings.massErrors = true
    }
  })

  it('shows the PSM table it is given right under the spectrum, above the ion table', async () => {
    const list = createRawSnippet(() => ({ render: () => '<div data-testid="the-table">PSMs</div>' }))
    const { target } = await show(PSM, { list })
    const box = target.querySelector('[data-testid="psm-list"]')!
    expect(box.querySelector('[data-testid="the-table"]')).not.toBeNull()
    const order = (a: Element, b: Element) => a.compareDocumentPosition(b) & Node.DOCUMENT_POSITION_FOLLOWING
    expect(order(target.querySelector('[data-testid="spectrum"]')!, box)).toBeTruthy()
    expect(order(box, target.querySelector('[data-testid="ion-table"]')!)).toBeTruthy()
  })

  it('accepts and rejects the PSM by its buttons and by A and R; again takes the mark away', async () => {
    const { view, target } = await show(PSM)
    const row = view.psm!.row
    const [accept, reject] = Array.from(target.querySelectorAll<HTMLButtonElement>('[data-testid="psm-curation"] button'))
    const press = (key: string) => {
      window.dispatchEvent(new KeyboardEvent('keydown', { key, bubbles: true, cancelable: true }))
      flushSync()
    }
    accept.click()
    flushSync()
    expect(view.curation[row]).toBe(1)
    expect(accept.getAttribute('aria-pressed')).toBe('true')
    press('r')
    expect(view.curation[row]).toBe(2)
    expect(accept.getAttribute('aria-pressed')).toBe('false')
    expect(reject.getAttribute('aria-pressed')).toBe('true')
    press('r')
    expect(view.curation[row]).toBe(0)
    expect(reject.getAttribute('aria-pressed')).toBe('false')
  })
})

describe("the PSM view's tools: Measure and Label", () => {
  let shown: Record<string, unknown> | null = null
  // a view as wide as the app's (happy-dom lays nothing out: every width is 0), so that the peaks stand apart
  const clientWidth = Object.getOwnPropertyDescriptor(HTMLElement.prototype, 'clientWidth')
  beforeAll(() => Object.defineProperty(HTMLElement.prototype, 'clientWidth', { configurable: true, get: () => 1200 }))
  afterAll(() => {
    if (clientWidth) Object.defineProperty(HTMLElement.prototype, 'clientWidth', clientWidth)
  })
  afterEach(() => {
    if (shown) unmount(shown)
    shown = null
    releaseResultsView()
    document.body.innerHTML = ''
  })

  async function show(sequence: string, onClose: () => void = () => {}) {
    const view = resultsViewFor(await open())
    view.psm = { row: rowOf(view, sequence), origin: { kind: 'psms' } }
    const target = document.createElement('div')
    document.body.append(target)
    shown = mount(PsmView, { target, props: { view, onClose, onBreadcrumb: () => {} } })
    flushSync()
    await settle(target)
    const { mz } = spectrumOf(await view.result.outcome.spectra.load(view.psm.row))
    return { view, target, mz }
  }

  /** The observed peaks of a loaded PSM. */
  function spectrumOf(loaded: Awaited<ReturnType<ResultsView['result']['outcome']['spectra']['load']>>) {
    const peaks = spectrumPeaks(loaded.spectra!, loaded.spectrumRow)!
    return { mz: peaks.mz }
  }

  /** The marks of the open PSM's spectrum (by its file and native ID). */
  const marksOf = (view: ResultsView) => {
    const { psms } = view.model.tables
    const row = view.psm!.row
    return view.psmMarks[`${psms.file[row]} ${psms.nativeId[row]}`]
  }

  const svgOf = (target: HTMLElement) => target.querySelector<SVGSVGElement>('[data-testid="spectrum"]')!

  /** Where the stick of the peak at `mz` ends on screen (from the SVG's m/z scale and its paths). */
  function peakAt(target: HTMLElement, mz: number): { x: number; y: number } {
    const svg = svgOf(target)
    const [d0, d1] = svg.getAttribute('data-mz-domain')!.split(' ').map(Number)
    const [r0, r1] = svg.getAttribute('data-x-range')!.split(' ').map(Number)
    const x = Math.round((r0 + ((mz - d0) * (r1 - r0)) / (d1 - d0)) * 10) / 10
    const paths = Array.from(svg.querySelectorAll('[data-testid="matched-peaks"], [data-testid="unmatched-peaks"]')).map((p) => p.getAttribute('d') ?? '')
    let y = Infinity
    for (const d of paths) for (const m of d.matchAll(/M(-?[\d.]+) (-?[\d.]+)V(-?[\d.]+)/g)) if (Number(m[1]) === x) y = Math.min(y, Number(m[3]))
    expect(y, `a stick at m/z ${mz}`).toBeLessThan(Infinity)
    return { x, y }
  }

  /** The sticks of the observed peaks: their x and where they end. */
  function sticksOf(target: HTMLElement): { x: number; y: number }[] {
    const paths = Array.from(svgOf(target).querySelectorAll('[data-testid="matched-peaks"], [data-testid="unmatched-peaks"]')).map((p) => p.getAttribute('d') ?? '')
    return paths.flatMap((d) => Array.from(d.matchAll(/M(-?[\d.]+) (-?[\d.]+)V(-?[\d.]+)/g), (m) => ({ x: Number(m[1]), y: Number(m[3]) })))
  }

  function clickAt(target: HTMLElement, p: { x: number; y: number }, type = 'click') {
    svgOf(target).dispatchEvent(new MouseEvent(type, { bubbles: true, clientX: p.x, clientY: p.y }))
    flushSync()
  }
  const key = (k: string, on: EventTarget = document.body) => {
    on.dispatchEvent(new KeyboardEvent('keydown', { key: k, bubbles: true, cancelable: true }))
    flushSync()
  }
  const button = (target: HTMLElement, id: string) => target.querySelector<HTMLButtonElement>(`[data-testid="${id}"]`)!

  /** Two observed peaks whose Δm/z is a residue's (Y, 163.0633), else any two far apart. */
  function pairOf(mz: Float32Array, delta = 163.06333): [number, number] {
    for (let i = 0; i < mz.length; i++) for (let j = i + 1; j < mz.length; j++) if (Math.abs(mz[j] - mz[i] - delta) < mz[j] * 10e-6) return [mz[i], mz[j]]
    throw new Error(`no two peaks ${delta} apart`)
  }

  it('measures two peaks: a bracket with their Δm/z, and what it may be', async () => {
    const { target, mz, view } = await show(PSM)
    const measureButton = button(target, 'tool-measure')
    expect(measureButton.getAttribute('aria-pressed')).toBe('false')
    expect(button(target, 'clear-measurements').disabled).toBe(true)
    measureButton.click()
    flushSync()
    expect(measureButton.getAttribute('aria-pressed')).toBe('true')
    expect(svgOf(target).style.cursor).toBe('crosshair')

    const [a, b] = pairOf(mz)
    const ionLabels = () => Array.from(target.querySelectorAll('[data-testid="peak-label"]')).map((l) => l.getAttribute('data-mz'))
    const before = ionLabels()
    clickAt(target, peakAt(target, a))
    expect(target.querySelector('[data-testid="measure-start"]')).not.toBeNull()
    expect(target.querySelector('[data-testid="tool-hint"]')?.textContent).toContain('second peak')
    // the pointer over the second peak: the dotted line and the live Δ
    const q = peakAt(target, b)
    svgOf(target).dispatchEvent(new PointerEvent('pointermove', { bubbles: true, clientX: q.x, clientY: q.y }))
    flushSync()
    expect(target.querySelector('[data-testid="measure-preview"] text')?.textContent).toBe(`Δ${(b - a).toFixed(3)}`)
    // (no tooltip while the live Δ shows)
    expect(target.querySelector('.pointer-events-none.absolute')).toBeNull()
    clickAt(target, q)

    const brackets = target.querySelectorAll('[data-testid="measurement"]')
    expect(brackets).toHaveLength(1)
    expect(brackets[0].getAttribute('data-text')).toBe(`Δ${(b - a).toFixed(3)} ≈ Y`)
    expect(brackets[0].querySelector('text')?.textContent).toBe(`Δ${(b - a).toFixed(3)} ≈ Y`)
    expect(target.querySelector('[data-testid="measure-start"]')).toBeNull()
    expect(svgOf(target).querySelector('desc')?.textContent).toContain(`Measured: Δm/z ${(b - a).toFixed(3)} ≈ Y between m/z ${a.toFixed(3)}`)
    expect(marksOf(view).measurements).toHaveLength(1)
    // the ions' labels stay, those of the measured peaks too; the dotted lines start above the labels of their peaks
    expect(ionLabels()).toEqual(before)
    for (const line of Array.from(brackets[0].querySelectorAll('line[stroke-dasharray]'))) {
      const lx = Number(line.getAttribute('x1'))
      const from = Number(line.getAttribute('y1'))
      for (const l of Array.from(target.querySelectorAll('[data-testid="peak-label"]'))) {
        if (Math.abs(Number(l.getAttribute('x')) - lx) < 1) expect(from, 'a dotted line through a label').toBeLessThanOrEqual(Number(l.getAttribute('y')) - 11)
      }
    }

    // in the SVG export (without the pointer's marks)
    const exported = exportSpectrumSvg(svgOf(target))
    expect(exported).toContain(`Δ${(b - a).toFixed(3)} ≈ Y`)
    expect(exported).not.toContain('data-testid')

    // a Δ that is nothing of the search: the Δ alone
    clickAt(target, peakAt(target, mz[0]))
    clickAt(target, peakAt(target, mz[mz.length - 1]))
    const texts = Array.from(target.querySelectorAll('[data-testid="measurement"]')).map((m) => m.getAttribute('data-text'))
    expect(texts[1]).toBe(`Δ${(mz[mz.length - 1] - mz[0]).toFixed(3)}`)

    // Clear Δ
    expect(button(target, 'clear-measurements').disabled).toBe(false)
    button(target, 'clear-measurements').click()
    flushSync()
    expect(target.querySelectorAll('[data-testid="measurement"]')).toHaveLength(0)
    expect(button(target, 'clear-measurements').disabled).toBe(true)
  })

  it('selects a bracket by a click, and Delete removes it; the measurements stay with their PSM', async () => {
    const { target, mz, view } = await show(PSM)
    button(target, 'tool-measure').click()
    flushSync()
    const [a, b] = pairOf(mz)
    clickAt(target, peakAt(target, a))
    clickAt(target, peakAt(target, b))
    // the bar: between the peaks, at the bracket's height
    const bar = target.querySelector('[data-testid="measurement"] line')!
    // (a point of the bar no peak reaches: a click at the top of a peak is the tool's)
    const [x1, x2] = [Number(bar.getAttribute('x1')), Number(bar.getAttribute('x2'))].sort((p, q) => p - q)
    const sticks = sticksOf(target)
    const free = Array.from({ length: Math.floor(x2 - x1) - 6 }, (_, i) => x1 + 3 + i).find((x) => sticks.every((p) => Math.abs(p.x - x) > 10))
    expect(free, 'a point of the bar away from the peaks').toBeDefined()
    const on = { x: free!, y: Number(bar.getAttribute('y1')) }
    clickAt(target, on)
    expect(target.querySelector('[data-testid="measurement"]')?.hasAttribute('data-selected')).toBe(true)
    // the export leaves the selection out: what has the colour of a selected one is the page's only, the bracket keeps
    // its own colour; one text of the bracket in the file
    const selectedColour = Array.from(svgOf(target).querySelectorAll('[fill*="measure-selected"], [stroke*="measure-selected"]'))
    expect(selectedColour.length).toBeGreaterThan(0)
    expect(selectedColour.every((el) => el.closest('[data-no-export]') !== null)).toBe(true)
    expect(target.querySelector('[data-testid="measurement"] > text')?.getAttribute('fill')).toBe('var(--plot-measure)')
    expect(exportSpectrumSvg(svgOf(target)).match(/<text[^>]*>Δ/g)).toHaveLength(1)
    // a second click deselects it, a third selects it again
    clickAt(target, on)
    expect(target.querySelector('[data-testid="measurement"]')?.hasAttribute('data-selected')).toBe(false)
    clickAt(target, on)
    expect(target.querySelector('[data-testid="measurement"]')?.hasAttribute('data-selected')).toBe(true)
    key('Delete')
    expect(target.querySelectorAll('[data-testid="measurement"]')).toHaveLength(0)

    // another PSM and back: the measurement is there again
    clickAt(target, peakAt(target, a))
    clickAt(target, peakAt(target, b))
    const row = view.psm!.row
    const other = rowOf(view, OTHER)
    view.psm = { row: other, origin: { kind: 'psms' } }
    flushSync()
    await settle(target)
    expect(target.querySelectorAll('[data-testid="measurement"]')).toHaveLength(0)
    view.psm = { row, origin: { kind: 'psms' } }
    flushSync()
    await settle(target)
    expect(target.querySelectorAll('[data-testid="measurement"]')).toHaveLength(1)
  })

  it('labels a peak, edits the label by a double click, and removes it by Delete or Clear labels', async () => {
    const { target, mz, view } = await show(PSM)
    const labelButton = button(target, 'tool-label')
    labelButton.click()
    flushSync()
    expect(labelButton.getAttribute('aria-pressed')).toBe('true')
    const [a, b] = pairOf(mz)
    clickAt(target, peakAt(target, a))
    let input = target.querySelector<HTMLInputElement>('[data-testid="label-editor"] input')!
    expect(input).not.toBeNull()
    expect(document.activeElement).toBe(input)
    input.value = 'my peak'
    input.dispatchEvent(new Event('input', { bubbles: true }))
    key('Enter', input)
    expect(target.querySelector('[data-testid="label-editor"]')).toBeNull()
    const labels = () => Array.from(target.querySelectorAll('[data-testid="user-label"]')).map((l) => l.textContent?.trim())
    expect(labels()).toEqual(['my peak'])
    expect(svgOf(target).querySelector('desc')?.textContent).toContain(`Your labels: “my peak” at m/z ${a.toFixed(4)}.`)
    expect(exportSpectrumSvg(svgOf(target))).toContain('my peak')

    // Esc cancels a label being written (and leaves the view open)
    clickAt(target, peakAt(target, b))
    input = target.querySelector<HTMLInputElement>('[data-testid="label-editor"] input')!
    input.value = 'not this'
    input.dispatchEvent(new Event('input', { bubbles: true }))
    key('Escape', input)
    expect(target.querySelector('[data-testid="label-editor"]')).toBeNull()
    expect(labels()).toEqual(['my peak'])
    expect(labelButton.getAttribute('aria-pressed')).toBe('true')

    // a double click on the label edits it
    const text = target.querySelector('[data-testid="user-label"] text')!
    const at = { x: Number(text.getAttribute('x')), y: Number(text.getAttribute('y')) - 4 }
    clickAt(target, at)
    clickAt(target, at, 'dblclick')
    input = target.querySelector<HTMLInputElement>('[data-testid="label-editor"] input')!
    expect(input.value).toBe('my peak')
    input.value = 'y3'
    input.dispatchEvent(new Event('input', { bubbles: true }))
    key('Enter', input)
    expect(labels()).toEqual(['y3'])

    // a click selects it, a second one deselects it; Delete removes it
    clickAt(target, at)
    expect(target.querySelector('[data-testid="user-label"]')?.hasAttribute('data-selected')).toBe(true)
    clickAt(target, at)
    expect(target.querySelector('[data-testid="user-label"]')?.hasAttribute('data-selected')).toBe(false)
    clickAt(target, at)
    expect(target.querySelector('[data-testid="user-label"]')?.hasAttribute('data-selected')).toBe(true)
    key('Backspace')
    expect(labels()).toEqual([])

    // Clear labels
    for (const p of [a, b]) {
      clickAt(target, peakAt(target, p))
      input = target.querySelector<HTMLInputElement>('[data-testid="label-editor"] input')!
      input.value = `at ${p.toFixed(1)}`
      input.dispatchEvent(new Event('input', { bubbles: true }))
      key('Enter', input)
    }
    expect(labels()).toHaveLength(2)
    expect(marksOf(view).labels).toHaveLength(2)
    expect(button(target, 'clear-labels').disabled).toBe(false)
    button(target, 'clear-labels').click()
    flushSync()
    expect(labels()).toEqual([])
    expect(button(target, 'clear-labels').disabled).toBe(true)
  })

  it('has one tool at a time; Esc cancels a half measurement, then leaves the tool, then closes the view', async () => {
    let closed = 0
    const { target, mz } = await show(PSM, () => closed++)
    const measureButton = button(target, 'tool-measure')
    const labelButton = button(target, 'tool-label')
    measureButton.click()
    flushSync()
    labelButton.click()
    flushSync()
    expect(measureButton.getAttribute('aria-pressed')).toBe('false')
    expect(labelButton.getAttribute('aria-pressed')).toBe('true')
    labelButton.click()
    flushSync()
    expect(labelButton.getAttribute('aria-pressed')).toBe('false')

    measureButton.click()
    flushSync()
    clickAt(target, peakAt(target, pairOf(mz)[0]))
    expect(target.querySelector('[data-testid="measure-start"]')).not.toBeNull()
    key('Escape')
    expect(target.querySelector('[data-testid="measure-start"]')).toBeNull()
    expect(measureButton.getAttribute('aria-pressed')).toBe('true')
    expect(closed).toBe(0)
    key('Escape')
    expect(measureButton.getAttribute('aria-pressed')).toBe('false')
    expect(svgOf(target).style.cursor).toBe('')
    expect(closed).toBe(0)
    key('Escape')
    expect(closed).toBe(1)
  })

  it('leaves the arrow keys, Backspace and M and L to the label being written', async () => {
    const { target, mz, view } = await show(PSM)
    const row = view.psm!.row
    button(target, 'tool-label').click()
    flushSync()
    // a label, selected
    const [a, b] = pairOf(mz)
    clickAt(target, peakAt(target, a))
    let input = target.querySelector<HTMLInputElement>('[data-testid="label-editor"] input')!
    input.value = 'keep me'
    input.dispatchEvent(new Event('input', { bubbles: true }))
    key('Enter', input)
    const text = target.querySelector('[data-testid="user-label"] text')!
    clickAt(target, { x: Number(text.getAttribute('x')), y: Number(text.getAttribute('y')) - 4 })
    expect(target.querySelector('[data-testid="user-label"]')?.hasAttribute('data-selected')).toBe(true)
    // another label being written
    clickAt(target, peakAt(target, b))
    input = target.querySelector<HTMLInputElement>('[data-testid="label-editor"] input')!
    const event = new KeyboardEvent('keydown', { key: 'ArrowRight', bubbles: true, cancelable: true })
    input.dispatchEvent(event)
    flushSync()
    expect(event.defaultPrevented).toBe(false)
    expect(view.psm!.row).toBe(row)
    // Backspace, Delete, M and L too: they write, they remove no selection and switch no tool
    for (const k of ['Backspace', 'Delete', 'm', 'l']) {
      const e = new KeyboardEvent('keydown', { key: k, bubbles: true, cancelable: true })
      input.dispatchEvent(e)
      flushSync()
      expect(e.defaultPrevented, k).toBe(false)
    }
    expect(button(target, 'tool-label').getAttribute('aria-pressed')).toBe('true')
    expect(marksOf(view).labels.map((l) => l.text)).toEqual(['keep me'])
  })

  it('switches the tools by M and L', async () => {
    const { target } = await show(PSM)
    key('m')
    expect(button(target, 'tool-measure').getAttribute('aria-pressed')).toBe('true')
    key('l')
    expect(button(target, 'tool-measure').getAttribute('aria-pressed')).toBe('false')
    expect(button(target, 'tool-label').getAttribute('aria-pressed')).toBe('true')
    key('L')
    expect(button(target, 'tool-label').getAttribute('aria-pressed')).toBe('false')
    expect(button(target, 'tool-measure').title).toContain('(M)')
    expect(button(target, 'tool-label').title).toContain('(L)')
  })

  it('says no half measurement once the PSM is another, and keeps the hint as wide', async () => {
    const { target, mz, view } = await show(PSM)
    button(target, 'tool-measure').click()
    flushSync()
    const hint = () => target.querySelector('[data-testid="tool-hint"]')
    const width = hint()?.className
    clickAt(target, peakAt(target, pairOf(mz)[0]))
    expect(hint()?.textContent).toContain('second peak')
    expect(hint()?.className).toBe(width)
    view.psm = { row: rowOf(view, OTHER), origin: { kind: 'psms' } }
    flushSync()
    expect(hint()?.textContent).toBe('Click two peaks for their Δm/z')
    await settle(target)
    expect(hint()?.textContent).toBe('Click two peaks for their Δm/z')
  })

  it("keeps the ions' labels clear of the user's, also of those that found no room", async () => {
    const { target, mz, view } = await show(PSM)
    const { psms } = view.model.tables
    const row = view.psm!.row
    // labels on the peak of the lowest ion label and its nine neighbours: more than fit above them
    const svg = svgOf(target)
    const lowest = Array.from(svg.querySelectorAll('[data-testid="peak-label"]')).sort((p, q) => Number(q.getAttribute('y')) - Number(p.getAttribute('y')))[0]
    const at = Array.from(mz).findIndex((v) => Math.abs(v - Number(lowest.getAttribute('data-mz'))) < 1e-3)
    const peaks = Array.from(mz).slice(Math.max(0, at - 5), at + 5)
    view.psmMarks[`${psms.file[row]} ${psms.nativeId[row]}`] = { measurements: [], labels: peaks.map((p) => ({ mz: p, intensity: 1, text: 'a long label of mine' })) }
    flushSync()
    const boxOf = (t: Element) => {
      const w = textWidth(t.textContent ?? '', 11) * 1.1
      const [x, y] = [Number(t.getAttribute('x')), Number(t.getAttribute('y'))]
      return { x0: x - w / 2, x1: x + w / 2, y0: y - 11, y1: y + 2 }
    }
    const own = Array.from(svg.querySelectorAll('[data-testid="user-label"] text')).map(boxOf)
    expect(own).toHaveLength(peaks.length)
    for (const l of Array.from(svg.querySelectorAll('[data-testid="peak-label"]')).map(boxOf)) {
      for (const o of own) expect(l.x0 < o.x1 && o.x0 < l.x1 && l.y0 < o.y1 && o.y0 < l.y1, 'an ion label on one of the user').toBe(false)
    }
  })

  it('keeps the marks per spectrum', async () => {
    const { target, mz, view } = await show(PSM)
    button(target, 'tool-measure').click()
    flushSync()
    const [a, b] = pairOf(mz)
    clickAt(target, peakAt(target, a))
    clickAt(target, peakAt(target, b))
    const { psms } = view.model.tables
    const row = view.psm!.row
    expect(Object.keys(view.psmMarks)).toEqual([`${psms.file[row]} ${psms.nativeId[row]}`])
  })
})
