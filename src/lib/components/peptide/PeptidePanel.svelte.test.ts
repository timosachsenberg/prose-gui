// @vitest-environment happy-dom
/// <reference types="node" />
// The peptide's detail on the app's example (src/lib/results/fixtures/example): what it says of the peptide, its
// proteins (the way up), its PSMs (each opens among the PSMs of the peptide), "PSMs of this peptide ›" (the level
// below) and Prev/Next through the rows of the Peptides table.
import { readFileSync } from 'node:fs'
import { basename, join } from 'node:path'
import { flushSync, mount, unmount } from 'svelte'
import { afterAll, afterEach, beforeAll, describe, expect, it, vi } from 'vitest'
import { input } from '../../design/fixtures'
import { resolveDesign } from '../../design/model'
import { peptidePsms } from '../../results/protein'
import { bytesOfRange, FileNotFound, readSearch } from '../../results/read'
import { releaseResultsView, resultsViewFor } from '../../results/view.svelte'
import type { OpenResult } from '../../search/session.svelte'
import PeptidePanel from './PeptidePanel.svelte'

// from the root of the project: under happy-dom, import.meta.url is not a file URL
const fixture = join(process.cwd(), 'src/lib/results/fixtures/example')

async function open(): Promise<OpenResult> {
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
    psmFdr: 0.02,
    proteinFdr: 1,
    settings: { parameters: {}, fixedMods: ['Carbamidomethyl (C)'], database: 'database.fasta', rescore: true, peptdeep: true, contaminants: true },
    outcome: { ...read, seconds: 1 },
  }
}

// happy-dom lays nothing out: every element would be 0 px high, so the PSM table would take each row as scrolled out
// of view. A 400 px grid, as in a window.
const clientHeight = Object.getOwnPropertyDescriptor(HTMLElement.prototype, 'clientHeight')
beforeAll(() => Object.defineProperty(HTMLElement.prototype, 'clientHeight', { configurable: true, get: () => 400 }))
afterAll(() => {
  if (clientHeight) Object.defineProperty(HTMLElement.prototype, 'clientHeight', clientHeight)
})

// a peptide of two proteins (tufA and tufB) with PSMs in both replicates
const SHARED = 'GYRPQFYFR'

describe("the peptide's detail", () => {
  let shown: Record<string, unknown> | null = null
  afterEach(() => {
    if (shown) unmount(shown)
    shown = null
    releaseResultsView()
    document.body.innerHTML = ''
  })

  async function show(sequence: string) {
    const view = resultsViewFor(await open())
    const peptide = view.model.tables.peptides.sequence.indexOf(sequence)
    expect(peptide, sequence).toBeGreaterThanOrEqual(0)
    view.tab = 'peptides'
    view.openPeptide(peptide)
    const calls = { onClose: vi.fn(), onNarrow: vi.fn(), onOpenProtein: vi.fn(), onOpenPsm: vi.fn() }
    const target = document.createElement('div')
    document.body.append(target)
    shown = mount(PeptidePanel, { target, props: { view, peptide, ...calls } })
    flushSync()
    const text = (id: string) => target.querySelector(`[data-testid="${id}"]`)?.textContent?.replace(/\s+/g, ' ').trim() ?? null
    return { view, peptide, target, text, calls }
  }

  it('says what the peptide is: its sequence, its PSMs, where it was found', async () => {
    const { view, peptide, target, text } = await show(SHARED)
    const psms = peptidePsms(view.model, peptide)
    expect(psms.length).toBeGreaterThan(1)
    expect(text('peptide-sequence')).toBe(SHARED)
    expect(text('peptide-psms')).toBe(`${psms.length} PSMs`)
    expect(text('peptide-found')).toBe('found in 2 of 2 samples')
    expect(target.querySelector('section')?.getAttribute('aria-label')).toBe(`Peptide ${view.model.tables.peptides.proforma[peptide]}`)
    // its place among the rows of the Peptides table
    const position = view.peptides.rows.indexOf(peptide)
    expect(text('detail-position')).toBe(`${position + 1} of ${view.peptides.rows.length}`)
  })

  it('lists its proteins with where it is in each; one opens in the Proteins tab', async () => {
    const { view, peptide, target, calls } = await show(SHARED)
    const proteins = Array.from(view.model.peptide.proteins[peptide])
    expect(proteins.length).toBe(2)
    const items = Array.from(target.querySelectorAll('[data-testid="peptide-proteins"] li'))
    expect(items.length).toBe(2)
    // the residues of the peptide in the protein: as long as the peptide
    for (const item of items) {
      const [from, to] = item.textContent!.match(/(\d+)–(\d+)\s*$/)!.slice(1).map(Number)
      expect(to - from + 1).toBe(SHARED.length)
    }
    items[1].querySelector('button')!.click()
    expect(calls.onOpenProtein).toHaveBeenCalledWith(proteins[1])
  })

  it('lists its PSMs, best q-value first; one opens, and the button goes on to the PSM table of the peptide', async () => {
    const { view, peptide, target, calls } = await show(SHARED)
    const psms = Array.from(peptidePsms(view.model, peptide))
    const rows = Array.from(target.querySelectorAll<HTMLElement>('[role="grid"] tbody tr[role="row"]'))
    expect(rows.length).toBe(psms.length)
    const q = view.model.tables.psms.qValue
    const shownQ = rows.map((r) => Number(r.querySelector('[data-column="qValue"]')!.textContent))
    expect(shownQ).toEqual([...shownQ].sort((a, b) => a - b))
    expect(shownQ[0]).toBeCloseTo(Math.min(...psms.map((i) => q[i])), 4)
    rows[1].click()
    expect(calls.onOpenPsm).toHaveBeenCalledTimes(1)
    expect(psms).toContain(calls.onOpenPsm.mock.calls[0][0])
    target.querySelector<HTMLButtonElement>('[data-testid="peptide-narrow"]')!.click()
    expect(calls.onNarrow).toHaveBeenCalledWith({ peptide })
  })

  it('Prev and Next go through the rows of the Peptides table; the cross closes', async () => {
    const { view, peptide, target, calls } = await show(SHARED)
    const rows = Array.from(view.peptides.rows)
    const at = rows.indexOf(peptide)
    const button = (label: string) => target.querySelector<HTMLButtonElement>(`button[aria-label="${label}"]`)!
    expect(button('Previous peptide').disabled).toBe(at === 0)
    button('Next peptide').click()
    flushSync()
    expect(view.peptidePanel).toBe(rows[at + 1])
    expect(view.peptides.selected).toBe(rows[at + 1])
    button("Close the peptide's detail").click()
    expect(calls.onClose).toHaveBeenCalledTimes(1)
  })
})
