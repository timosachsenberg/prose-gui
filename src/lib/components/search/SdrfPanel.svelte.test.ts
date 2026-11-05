// @vitest-environment happy-dom
// The SDRF in New search, as a user goes through it (the shim's commands replaced): an SDRF is loaded and its
// files resolved, the panel says what is missing, the search waits until it is searched without it.
import { readFileSync } from 'node:fs'
import { flushSync, mount, unmount } from 'svelte'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import type { FileFacts } from '$lib/desktop'

const facts = (path: string): FileFacts => ({
  path,
  name: path.split('/').pop()!,
  size: 1000,
  spectra: 139,
  proteins: null,
  residues: null,
  error: null,
  problem: null,
  warnings: [],
  instrument: { name: 'LTQ Orbitrap XL', accession: 'MS:1000556' },
})

const shim = vi.hoisted(() => ({
  located: [] as { exact: string | null; mzml: string | null }[],
  /** Replaces `located` when set: the look-up's answer for these names. */
  locate: null as null | ((names: string[]) => Promise<{ exact: string | null; mzml: string | null }[]>),
  asked: [] as string[][],
  chosen: null as string | null,
}))
vi.mock('$lib/desktop', () => ({
  inspectFiles: vi.fn(async (paths: string[]) => paths.map(facts)),
  locateDataFiles: vi.fn(async (_sdrf: string, names: string[]) => {
    shim.asked.push(names)
    return shim.locate ? shim.locate(names) : shim.located
  }),
  chooseFolder: vi.fn(async () => '/data'),
  chooseFile: vi.fn(async () => shim.chosen),
  openDesignFile: vi.fn(async () => null),
  isDesktop: () => true,
  errorText: (e: unknown) => String(e),
}))

const { searchForm: form } = await import('$lib/search/form.svelte')
const { sdrfSession: sdrf } = await import('$lib/design/sdrf-session.svelte')
const SdrfPanel = (await import('./SdrfPanel.svelte')).default

// (from the root of the project: under happy-dom, import.meta.url is not a file URL)
const ecoli = readFileSync('src/lib/design/sdrf-fixtures/ecoli.sdrf.tsv', 'utf8')
let shown: Record<string, unknown>[] = []

beforeEach(async () => {
  form.files = []
  form.notices = []
  sdrf.clear()
  shim.located = [
    { exact: '/sdrf/Ecoli_1.mzML', mzml: '/sdrf/Ecoli_1.mzML' },
    { exact: null, mzml: '/sdrf/Ecoli_2.mzML' },
    { exact: null, mzml: null },
  ]
  shim.locate = null
  shim.asked = []
  shim.chosen = null
  await sdrf.load('/sdrf/PXD000001_ecoli.sdrf.tsv', ecoli)
})
afterEach(() => {
  for (const s of shown) unmount(s)
  shown = []
  document.body.innerHTML = ''
})

const text = () => document.body.textContent ?? ''

describe('an SDRF in New search', () => {
  it('adds the files found, with their design and instrument, and says what is missing', async () => {
    expect(form.files.map((r) => [r.file.name, r.condition, r.replicate, r.instrument])).toEqual([
      ['Ecoli_1.mzML', 'wild type', 1, 'NT=LTQ Orbitrap XL;AC=MS:1000556'],
      ['Ecoli_2.mzML', 'wild type', 2, 'NT=LTQ Orbitrap XL;AC=MS:1000556'],
    ])
    expect(sdrf.files.map((f) => f.status)).toEqual(['found', 'mzml', 'missing'])
    expect(sdrf.problem).toContain('Ecoli_3.mzML')
    shown.push(mount(SdrfPanel, { target: document.body }))
    flushSync()
    expect(text()).toContain('2 of 3 data files found')
    expect(text()).toContain('Ecoli_3.mzML')
    const skip = document.querySelector<HTMLInputElement>('[data-testid="sdrf-panel"] input[type="checkbox"]')!
    skip.click()
    flushSync()
    expect(sdrf.problem).toBeNull()
  })

  it('a look-up of an earlier SDRF changes nothing of the one loaded since', async () => {
    form.files = []
    let answerA!: (v: { exact: string | null; mzml: string | null }[]) => void
    shim.locate = () => new Promise((resolve) => (answerA = resolve))
    const a = sdrf.load('/a/A.sdrf.tsv', ecoli)
    shim.locate = async (names) => names.map(() => ({ exact: null, mzml: null }))
    await sdrf.load('/b/B.sdrf.tsv', ecoli.replace(/Ecoli_/g, 'Other_'))
    answerA([
      { exact: '/a/Ecoli_1.mzML', mzml: null },
      { exact: '/a/Ecoli_2.raw', mzml: null },
      { exact: null, mzml: null },
    ])
    await a
    expect(sdrf.name).toBe('B.sdrf.tsv')
    expect(sdrf.files.map((f) => [f.name, f.path])).toEqual([
      ['Other_1.mzML', null],
      ['Other_2.raw', null],
      ['Other_3.mzML', null],
    ])
    expect(form.files).toEqual([])
  })

  it('gives its design to files that were in the table before it was loaded', async () => {
    sdrf.clear()
    form.files = []
    await form.addFiles(['/sdrf/Ecoli_1.mzML'])
    expect(form.files[0].condition).toBeUndefined()
    await sdrf.load('/sdrf/ecoli.sdrf.tsv', ecoli)
    expect(form.files.map((r) => [r.file.name, r.condition, r.replicate])).toEqual([
      ['Ecoli_1.mzML', 'wild type', 1],
      ['Ecoli_2.mzML', 'wild type', 2],
    ])
    // a later look-up (Locate folder…) leaves what the user changed since
    form.setField(form.files[0].id, 'condition', 'changed')
    await sdrf.load('/sdrf/ecoli.sdrf.tsv', ecoli)
    expect(form.files[0].condition).toBe('wild type') // (a new load is a new SDRF: its design again)
  })

  it('Locate… of a Bruker .d takes the folder of the analysis.tdf chosen', async () => {
    shim.chosen = '/data/run3.d/analysis.tdf'
    await sdrf.locateFile('Ecoli_3.mzML')
    expect(sdrf.files[2].path).toBe('/data/run3.d')
  })
})
