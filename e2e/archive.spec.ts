// Results archives through the app, without a search (no ProSE needed): a saved search (the reader's example
// fixture, a search of the pinned ProSE, with a record) is opened from History, written as a results archive
// (Export; the save dialog answered by PROSE_TEST_DIALOG_PATH, e2e/app.sh), deleted, and the archive opened again
// (History › Import results archive: the Rust side unpacks the file entry by entry, `import_archive_file`). The
// search comes back as one of its own with the same PSMs; a PSM's spectrum, prediction and features (read from its
// input's files when it is opened) are there. A file that is no archive is refused and leaves no folder.
import { cpSync, existsSync, readFileSync, rmSync, writeFileSync } from 'node:fs'
import { join, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'
import { resolveDesign } from '../src/lib/design/model.ts'
import { appReady, clearSearches, dataHome, goTo, openFirstPsm, searchFolders, searchesDir, textOf } from './lib.ts'

const FIXTURE = resolve(fileURLToPath(new URL('../src/lib/results/fixtures/example', import.meta.url)))
const ID = '8000-0'

describe('a results archive, written and opened by the app', () => {
  const file = () => join(dataHome(), 'chosen.prose')
  let count = ''

  before(async () => {
    await appReady()
    clearSearches()
    rmSync(file(), { force: true })
    // the fixture as a saved search of History
    cpSync(FIXTURE, join(searchesDir(), ID), { recursive: true })
    const files = ['Ecoli_1.mzML', 'Ecoli_2.mzML']
    const record = {
      schema: 1,
      id: ID,
      name: 'Archived example',
      createdAt: '2026-10-21T12:00:00.000Z',
      appVersion: '0.5.0',
      toolVersion: 'fixture',
      files,
      database: 'Ecoli_K12_proteome.fasta',
      design: resolveDesign(files.map((fileName, i) => ({ id: `f${i + 1}`, fileName, fileSize: 1_200_000 }))),
      settings: { parameters: {}, fixedMods: [], rescore: true, peptdeep: true, contaminants: true },
      psms: 132,
      psmFdr: 0.02,
      proteinFdr: 0.01,
      seconds: 3,
    }
    writeFileSync(join(searchesDir(), ID, 'search.json'), JSON.stringify(record))
  })

  after(() => {
    clearSearches()
    rmSync(file(), { force: true })
  })

  it('is written from an open result', async () => {
    await goTo('History')
    const row = $(`[data-testid="history-row"][data-id="${ID}"]`)
    await row.waitForDisplayed({ timeout: 30000 })
    await row.$('button=Open').click()
    await $('[data-testid="psm-count"]').waitForDisplayed({ timeout: 30000 })
    count = await textOf('[data-testid="psm-count"]')
    expect(count).toMatch(/[1-9]/)

    await $('header').$('button*=Export').click()
    await $('[role="menuitem"]*=Results archive (.prose)').click()
    await browser.waitUntil(() => existsSync(file()) && readFileSync(file()).length > 0, { timeoutMsg: 'no results archive was written' })
    // a zip: a plain one (a search this small), which every version of the app opens
    const bytes = readFileSync(file())
    expect(bytes.subarray(0, 2).toString('latin1')).toBe('PK')
    expect(bytes.readUInt32LE(bytes.length - 22)).toBe(0x06054b50)
    expect(bytes.readUInt16LE(bytes.length - 22 + 10)).toBeLessThan(0xffff)
  })

  it('opens again as a search of its own, with the same PSMs, their spectra and features', async () => {
    await goTo('History')
    await $(`[data-testid="history-row"][data-id="${ID}"]`).$('button[aria-label^="Delete"]').click()
    await $('[role="dialog"]').$('button=Delete').click()
    await $('[data-testid="history-empty"]').waitForDisplayed()
    expect(searchFolders().finished).toEqual([])

    await $('[data-testid="import-archive"]').click()
    await $('[data-testid="psm-count"]').waitForDisplayed({ timeout: 30000 })
    expect(await textOf('[data-testid="psm-count"]')).toBe(count)
    // one folder, of a new id, with the record marked as imported and nothing left beside it
    const { finished, partial } = searchFolders()
    expect(partial).toEqual([])
    expect(finished.length).toBe(1)
    expect(finished[0]).not.toBe(ID)
    const record = JSON.parse(readFileSync(join(searchesDir(), finished[0], 'search.json'), 'utf8'))
    expect(record).toMatchObject({ id: finished[0], imported: true, name: 'Archived example' })
    // the files as they were
    for (const name of ['qpx/quantms.psm.parquet', 'qpx/runs/1__Ecoli_1.psm.parquet', 'qpx/runs/1__Ecoli_1.mz.parquet']) {
      expect(readFileSync(join(searchesDir(), finished[0], name)).equals(readFileSync(join(FIXTURE, name)))).toBe(true)
    }

    // a PSM: its spectrum and prediction, and its features, which are read with them
    await openFirstPsm()
    await expect($('[data-testid="bottom-label"]')).toHaveText(expect.stringContaining('PeptDeep'))
    const features = $('[data-testid="psm-features"]')
    await browser.waitUntil(async () => (await textOf('[data-testid="psm-features"]')).includes('ln(hyperscore)'), {
      timeoutMsg: 'the PSM view shows no ln(hyperscore): the features of the PSM were not read',
    })
    await expect(features).toHaveText(expect.stringContaining('ms2_cosine'))
    await expect(features).toHaveText(expect.stringContaining('Missed cleavages'))
  })

  it('a file that is no results archive is refused, and leaves no folder', async () => {
    const before = searchFolders()
    writeFileSync(file(), 'hello, this is text and not a zip file at all')
    await goTo('History')
    await $('[data-testid="import-archive"]').click()
    await expect($('[data-testid="history-problem"]')).toHaveText(expect.stringContaining('not a zip file'))
    expect(searchFolders()).toEqual(before)
    // a zip cut short
    const zip = Buffer.concat([Buffer.from('PK\u0003\u0004', 'latin1'), Buffer.alloc(64)])
    writeFileSync(file(), zip)
    await $('[data-testid="import-archive"]').click()
    await expect($('[data-testid="history-problem"]')).toHaveText(expect.stringContaining('zip directory cannot be read'))
    expect(searchFolders()).toEqual(before)
  })
})
