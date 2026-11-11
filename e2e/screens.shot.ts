// Screenshots of the app for docs/screens, at the web app's screenshot size (1440×900), to compare
// with the web app's (docs/screens/compare.html). Not part of the e2e run (the name has no `.spec.`):
//   PROSE_BIN=… e2e/run.sh e2e/screens.shot.ts
// Runs the example as the web app's screenshots did (PeptDeep on), then the views (14: the PSM view's Measure and
// Label in use, 15: the same in the dark theme); removes its search folder at the end.
import { mkdirSync, writeFileSync } from 'node:fs'
import { fileURLToPath } from 'node:url'
import { resolve } from 'node:path'
import { clearSearches, clickSpectrum, jsClick, residuePair, tallestPeakAwayFrom } from './lib.ts'

const dir = resolve(fileURLToPath(new URL('../docs/screens', import.meta.url)))

async function shot(name: string) {
  // the pointer away from buttons and rows (no hover), and a moment for the paint
  await browser.pause(400)
  mkdirSync(dir, { recursive: true })
  writeFileSync(resolve(dir, `${name}.png`), Buffer.from(await browser.takeScreenshot(), 'base64'))
}

const text = async (selector: string) => String(await $(selector).getProperty('textContent'))

/** Measure on, a measurement started at `from`, the pointer on `to` (the live Δ). */
async function halfMeasurement(from: { x: number; y: number }, to: { x: number; y: number }) {
  await $('[data-testid="tool-measure"]').click()
  await clickSpectrum(from)
  await $('[data-testid="measure-start"]').waitForExist({ timeout: 5000 })
  const box = await browser.execute(() => {
    const r = document.querySelector('[data-testid="spectrum"]')!.getBoundingClientRect()
    return { left: r.left, top: r.top }
  })
  await browser
    .action('pointer', { parameters: { pointerType: 'mouse' } })
    .move({ origin: 'viewport', x: Math.round(box.left + to.x), y: Math.round(box.top + to.y) })
    .perform()
  await $('[data-testid="measure-preview"]').waitForExist({ timeout: 5000 })
}

/** A measurement from `from` to `to`, a label at `labelAt`, then a measurement half made from `labelAt` back to `from`. */
async function makeMarks(from: { x: number; y: number }, to: { x: number; y: number }, labelAt: { x: number; y: number }) {
  await $('[data-testid="tool-measure"]').click()
  await clickSpectrum(from)
  await $('[data-testid="measure-start"]').waitForExist({ timeout: 5000 })
  await clickSpectrum(to)
  await $('[data-testid="measurement"]').waitForExist({ timeout: 5000 })
  await $('[data-testid="tool-label"]').click()
  await clickSpectrum(labelAt)
  await $('[data-testid="label-editor"] input').waitForExist({ timeout: 5000 })
  await $('[data-testid="label-editor"] input').setValue('check this peak')
  await browser.keys('Enter')
  await $('[data-testid="user-label"]').waitForExist({ timeout: 5000 })
  await browser.keys('Escape') // the Label tool
  await halfMeasurement(labelAt, from)
}

describe('screens', () => {
  after(() => clearSearches())

  it('takes the screenshots', async () => {
    await $('header').waitForDisplayed({ timeout: 20000 })
    await browser.setWindowSize(1440, 900).catch(() => {})
    const size = await browser.execute(() => [window.innerWidth, window.innerHeight])
    console.log('viewport', size)
    await shot('01-new-search-empty')

    await $('button=Try the example').click()
    await browser.waitUntil(async () => (await text('[data-testid="fasta-proteins"]')).includes('4,136 proteins'), { timeout: 20000 })
    // as the web app's screenshots: PeptDeep on
    await $('#peptdeep').click()
    await browser.execute(() => window.scrollTo(0, 0))
    await shot('02-new-search-files')

    await $('button*=Start search').click()
    await $('[data-testid="search-progress"]').waitForDisplayed({ timeout: 20000 })
    await browser.waitUntil(async () => /Searching/.test(await text('[data-testid="search-progress"]')), { timeout: 60000, interval: 200 })
    await shot('03-search-running')

    await browser.waitUntil(async () => /#\/results\/[^/]+$/.test(await browser.getUrl()), { timeout: 180000, interval: 500 })
    await $('[role="grid"][aria-label="Proteins"]').waitForDisplayed({ timeout: 20000 })
    await shot('04-results-proteins')

    // the first protein's panel, its peptides
    await jsClick((await $$('[role="grid"][aria-label="Proteins"] tbody tr[aria-rowindex]'))[0])
    await $('[data-testid="protein-panel"]').waitForDisplayed({ timeout: 10000 })
    // (its tab, not the button "Peptides of … ›" that goes on to the Peptides tab)
    const peptidesTab = await $('[data-testid="protein-panel"]').$('button[role="tab"]*=Peptides')
    if (await peptidesTab.isExisting()) await jsClick(peptidesTab)
    await shot('05-protein-panel')
    await browser.keys('Escape')

    // the PSM view of LHVNSAR (the web app's screenshot), else the first PSM
    await jsClick(await $('[role="tablist"]').$('button*=PSMs'))
    const grid = $('[role="grid"][aria-label="PSMs"]')
    await grid.waitForDisplayed({ timeout: 10000 })
    const rows = await grid.$$('tbody tr[aria-rowindex]')
    let target = rows[0]
    for (const r of rows) if (String(await r.getProperty('textContent')).includes('LHVNSAR')) target = r
    await jsClick(target)
    await $('[data-testid="psm-view"]').waitForDisplayed({ timeout: 10000 })
    await shot('06-psm-mirror')

    // the tools in use: a measurement between two annotated peaks a residue apart ("≈ residue"), a label of one's own
    // on another peak, and Measure on with a second measurement half made (the pointer on its second peak: the live Δ);
    // kept for the dark theme's (15), then cleared
    const [from, to] = await residuePair()
    const away = await tallestPeakAwayFrom([from, to])
    await makeMarks(from, to, away)
    await shot('14-psm-measure')
    await browser.keys('Escape') // the half measurement
    await browser.keys('Escape') // the tool

    // the same in the dark theme (the top bar's menu); then back to the system's
    await jsClick(await $('[data-testid="theme-menu"]'))
    await jsClick(await $('[data-testid="theme-dark"]'))
    await browser.waitUntil(async () => await browser.execute(() => document.documentElement.classList.contains('dark')), { timeout: 5000 })
    await halfMeasurement(away, to)
    await shot('15-psm-measure-dark')
    await browser.keys('Escape')
    await browser.keys('Escape')
    await jsClick(await $('[data-testid="clear-measurements"]'))
    await jsClick(await $('[data-testid="clear-labels"]'))
    await jsClick(await $('[data-testid="theme-menu"]'))
    await jsClick(await $('[data-testid="theme-system"]'))
    await browser.action('pointer').move({ origin: 'viewport', x: 2, y: 2 }).perform()
    await browser.keys('Escape')

    await jsClick(await $('[role="tablist"]').$('button=Overview'))
    await $('[data-testid="search-facts"]').waitForDisplayed({ timeout: 10000 })
    await shot('07-overview')

    await jsClick(await $('[role="tablist"]').$('button=QC'))
    await browser.pause(300)
    await shot('08-qc')

    await $('a=History').click()
    await $('[data-testid="view-history"]').waitForDisplayed({ timeout: 10000 })
    await shot('10-history')
  })
})
