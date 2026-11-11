// Going between the proteins, the peptides and the PSMs of a result (the bundled example): the tabs and the filter row
// stay where they are whatever is open, a click opens the detail next to its table, Enter (or a double click)
// narrows the level below, the chips of the filter row say to what, and Backspace goes up.
import { expect, openFixture, test } from './fixtures'
import type { Locator, Page } from '@playwright/test'
import { readFileSync } from 'node:fs'

test.beforeEach(async ({ page }) => {
  await openFixture(page)
})

const tab = (page: Page, name: 'Overview' | 'Proteins' | 'Peptides' | 'PSMs') => page.getByTestId('results-tabs').getByRole('tab', { name: new RegExp(`^${name}`) })
const rows = (page: Page) => page.getByTestId('results-table-area').locator('tbody tr[role="row"]')
/** A click on a row, in its first cell (the accession in the middle of a row is a link to the protein). */
const clickRow = (row: Locator) => row.locator('td').first().click()
const top = async (locator: Locator) => Math.round((await locator.boundingBox())!.y)
const position = (page: Page) => page.getByTestId('results-detail').getByTestId('detail-position')

/** What scrolls in the PSM view, and whether the page does. */
const scrolling = (page: Page) =>
  page.evaluate(() => {
    const view = document.querySelector('[data-testid="psm-view"]')!
    const scrolls = (el: Element) => el.scrollHeight > el.clientHeight + 2 && /auto|scroll/.test(getComputedStyle(el).overflowY)
    return {
      view: [view, ...view.querySelectorAll('*')].filter(scrolls).map((el) => el.getAttribute('data-testid') ?? el.tagName),
      page: document.documentElement.scrollHeight > window.innerHeight,
    }
  })

test('the tabs, the filter row and the table stay where they are, whatever is open', async ({ page }) => {
  const tabs = page.getByTestId('results-tabs')
  const filters = page.getByTestId('filter-bar')
  const table = page.getByTestId('results-table-area').getByRole('grid')
  const seen: { what: string; tabs: number; filters: number; table: number }[] = []
  const note = async (what: string) => {
    await expect(tabs).toBeVisible()
    seen.push({ what, tabs: await top(tabs), filters: await top(filters), table: await top(table) })
  }

  await tab(page, 'Proteins').click()
  await note('the proteins')
  await clickRow(rows(page).first())
  await expect(page.getByTestId('protein-panel')).toBeVisible()
  await note('a protein open')

  await tab(page, 'Peptides').click()
  await note('the peptides')
  await clickRow(rows(page).first())
  await expect(page.getByTestId('peptide-panel')).toBeVisible()
  await note('a peptide open')

  await tab(page, 'PSMs').click()
  await note('the PSMs')
  await clickRow(rows(page).first())
  await expect(page.getByTestId('psm-view')).toBeVisible()
  await note('a PSM open')

  for (const s of seen) expect({ ...s, what: undefined }, s.what).toEqual({ ...seen[0], what: undefined })
})

test('Enter on the open protein narrows the tables below to it; its chip widens them again', async ({ page }) => {
  await tab(page, 'Proteins').click()
  await clickRow(rows(page).nth(1))
  const panel = page.getByTestId('protein-panel')
  await expect(panel).toBeVisible()
  const accession = (await panel.getByTestId('panel-accession').innerText()).trim()
  const peptides = Number(/\d+/.exec(await panel.getByRole('tab', { name: /^Peptides/ }).innerText())![0])
  const psms = Number(/\d+/.exec(await panel.getByTestId('panel-psms').innerText())![0])

  // the row is open: Enter goes on to its peptides
  await page.keyboard.press('Enter')
  await expect(tab(page, 'Peptides')).toHaveAttribute('aria-selected', 'true')
  await expect(page.getByTestId('chip-protein')).toContainText(accession)
  await expect(rows(page)).toHaveCount(peptides)
  await expect(tab(page, 'Peptides').getByTestId('tab-narrowed')).toHaveText(`${peptides} of 54`)
  // the PSMs are narrowed with them: one narrowing for both tables
  await expect(tab(page, 'PSMs').getByTestId('tab-narrowed')).toHaveText(`${psms} of 132`)
  await tab(page, 'PSMs').click()
  await expect(page.getByTestId('chip-protein')).toContainText(accession)
  await expect(rows(page)).toHaveCount(psms)

  // without the chip, all PSMs again (and all peptides)
  await page.getByTestId('chip-protein').click()
  await expect(page.getByTestId('chip-protein')).toHaveCount(0)
  await expect(page.getByTestId('tab-narrowed')).toHaveCount(0)
  await expect(page.getByTestId('row-count')).toHaveText('132 of 132')
  // the Proteins tab still has the protein open
  await tab(page, 'Proteins').click()
  await expect(panel.getByTestId('panel-accession')).toHaveText(accession)
})

test('a double click on a protein, and the buttons of its panel, narrow as Enter does', async ({ page }) => {
  await tab(page, 'Proteins').click()
  await rows(page).nth(2).locator('td').first().dblclick()
  await expect(tab(page, 'Peptides')).toHaveAttribute('aria-selected', 'true')
  await expect(page.getByTestId('chip-protein')).toBeVisible()

  await tab(page, 'Proteins').click()
  const panel = page.getByTestId('protein-panel')
  const accession = (await panel.getByTestId('panel-accession').innerText()).trim()
  await panel.getByRole('button', { name: `PSMs of ${accession} ›` }).click()
  await expect(tab(page, 'PSMs')).toHaveAttribute('aria-selected', 'true')
  await expect(page.getByTestId('chip-protein')).toContainText(accession)
})

test('a PSM opened through a peptide: Prev and Next go through the peptide\'s PSMs, and through all once its chip is gone', async ({ page }) => {
  await tab(page, 'Peptides').click()
  await clickRow(rows(page).first())
  const detail = page.getByTestId('peptide-panel')
  await expect(detail).toBeVisible()
  const sequence = (await detail.getByTestId('peptide-sequence').innerText()).trim()
  const own = Number(/\d+/.exec(await detail.getByTestId('peptide-psms').innerText())![0])

  // Enter: the PSMs of the peptide
  await page.keyboard.press('Enter')
  await expect(tab(page, 'PSMs')).toHaveAttribute('aria-selected', 'true')
  await expect(page.getByTestId('chip-peptide')).toBeVisible()
  await expect(rows(page)).toHaveCount(own)
  await clickRow(rows(page).first())
  const view = page.getByTestId('psm-view')
  await expect(view).toBeVisible()
  await expect(view.getByTestId('psm-up')).toContainText(sequence)
  await expect(position(page)).toHaveText(`1 of ${own}`)
  // the tabs are still there, and the chips say why the table is short
  await expect(page.getByTestId('results-tabs')).toBeVisible()
  await expect(page.getByTestId('narrowing')).toBeVisible()

  // the chips go: the same PSM stays open, now one of all PSMs, its row in sight and the keys in the table
  for (const chip of ['chip-peptide', 'chip-protein']) {
    if ((await page.getByTestId(chip).count()) > 0) await page.getByTestId(chip).click()
  }
  await expect(page.getByTestId('narrowing')).toHaveCount(0)
  await expect(view.getByTestId('psm-up')).toContainText(sequence)
  await expect(position(page)).toHaveText(/^\d+ of 132$/)
  const at = Number(/^\d+/.exec(await position(page).innerText())![0])
  await expect(page.getByTestId('results-table-area').locator('tbody tr[aria-selected="true"]')).toBeInViewport()
  await page.keyboard.press(at < 132 ? 'ArrowDown' : 'ArrowUp')
  await expect(position(page)).toHaveText(`${at < 132 ? at + 1 : at - 1} of 132`)
  await view.getByRole('button', { name: at < 132 ? 'Previous PSM' : 'Next PSM' }).click()
  await expect(position(page)).toHaveText(`${at} of 132`)
  await expect(view.getByTestId('psm-up')).toContainText(sequence)
})

test('Backspace goes up: from a PSM to its peptide, from the peptide to its protein', async ({ page }) => {
  await tab(page, 'PSMs').click()
  await clickRow(rows(page).nth(3))
  const view = page.getByTestId('psm-view')
  await expect(view).toBeVisible()
  const up = view.getByTestId('psm-up').getByRole('button')
  const peptide = (await up.first().innerText()).replace(/^peptide\s+/, '').replace(/\s*↑$/, '')
  const protein = (await up.nth(1).innerText()).replace(/^protein\s+/, '').replace(/\s*↑$/, '')

  await page.keyboard.press('Backspace')
  await expect(tab(page, 'Peptides')).toHaveAttribute('aria-selected', 'true')
  await expect(page.getByTestId('peptide-panel').getByTestId('peptide-sequence')).toHaveText(peptide)
  await expect(page.getByTestId('results-table-area').locator('tbody tr[aria-selected="true"]')).toBeInViewport()

  await page.keyboard.press('Backspace')
  await expect(tab(page, 'Proteins')).toHaveAttribute('aria-selected', 'true')
  await expect(page.getByTestId('protein-panel').getByTestId('panel-accession')).toHaveText(protein)

  // and the PSM is still open where it was left
  await tab(page, 'PSMs').click()
  await expect(view.getByTestId('psm-up')).toContainText(peptide)
})

test('the ways up of the PSM view go to the peptide and to the protein', async ({ page }) => {
  await tab(page, 'PSMs').click()
  await clickRow(rows(page).first())
  const up = page.getByTestId('psm-view').getByTestId('psm-up').getByRole('button')
  const protein = (await up.nth(1).innerText()).replace(/^protein\s+/, '').replace(/\s*↑$/, '')
  await up.nth(1).click()
  await expect(tab(page, 'Proteins')).toHaveAttribute('aria-selected', 'true')
  await expect(page.getByTestId('protein-panel').getByTestId('panel-accession')).toHaveText(protein)
})

test('a peptide\'s detail: its proteins and PSMs, each a way on', async ({ page }) => {
  await tab(page, 'Peptides').click()
  await clickRow(rows(page).first())
  const detail = page.getByTestId('peptide-panel')
  await expect(detail).toBeVisible()
  // the arrow keys of the table go through the peptides, the detail follows
  const first = await detail.getByTestId('peptide-sequence').innerText()
  await expect(position(page)).toHaveText('1 of 54')
  await page.keyboard.press('ArrowDown')
  await expect(position(page)).toHaveText('2 of 54')
  await expect(detail.getByTestId('peptide-sequence')).not.toHaveText(first)

  // a PSM of the detail opens in the PSMs tab, among the PSMs of the peptide
  const sequence = (await detail.getByTestId('peptide-sequence').innerText()).trim()
  await clickRow(detail.locator('tbody tr[role="row"]').first())
  await expect(tab(page, 'PSMs')).toHaveAttribute('aria-selected', 'true')
  await expect(page.getByTestId('chip-peptide')).toBeVisible()
  await expect(page.getByTestId('psm-view').getByTestId('psm-up')).toContainText(sequence)

  // a protein of the detail opens in the Proteins tab
  await tab(page, 'Peptides').click()
  const protein = detail.getByTestId('peptide-proteins').getByRole('button').first()
  const accession = (await protein.innerText()).trim().split(/\s/)[0]
  await protein.click()
  await expect(tab(page, 'Proteins')).toHaveAttribute('aria-selected', 'true')
  await expect(page.getByTestId('protein-panel').getByTestId('panel-accession')).toHaveText(accession)
})

for (const size of [
  { width: 1440, height: 900 },
  { width: 1280, height: 720 },
  { width: 1024, height: 768 },
]) {
  test(`the PSM view fits under its table without a scroll bar (${size.width} × ${size.height})`, async ({ page }) => {
    await page.setViewportSize(size)
    await tab(page, 'PSMs').click()
    await clickRow(rows(page).first())
    const view = page.getByTestId('psm-view')
    await expect(view.getByTestId('spectrum')).toBeVisible()
    for (let i = 0; i < 4; i++) {
      expect(await scrolling(page), `PSM ${i + 1}`).toEqual({ view: [], page: false })
      // the drawing ends within the view
      const drawing = (await view.getByTestId('spectrum').boundingBox())!
      const box = (await view.boundingBox())!
      expect(drawing.y + drawing.height).toBeLessThanOrEqual(box.y + box.height + 1)
      await page.keyboard.press('ArrowDown')
    }
    // the table above it keeps some rows
    expect(await rows(page).count()).toBeGreaterThanOrEqual(3)
    await expect(page.getByTestId('results-tabs')).toBeVisible()
  })
}

test('the mass errors show under the spectrum in the default window, and give way to it in a low one', async ({ page }) => {
  await page.setViewportSize({ width: 1440, height: 900 })
  await tab(page, 'PSMs').click()
  await clickRow(rows(page).first())
  const view = page.getByTestId('psm-view')
  await expect(view.getByTestId('error-dot').first()).toBeVisible()
  await page.setViewportSize({ width: 1440, height: 680 })
  await expect(view.getByTestId('error-dot')).toHaveCount(0)
  await expect(view.getByTestId('spectrum')).toBeVisible()
  await page.setViewportSize({ width: 1440, height: 900 })
  await expect(view.getByTestId('error-dot').first()).toBeVisible()
})

test('the line between a table and its detail sizes them, and the size is kept', async ({ page }) => {
  await tab(page, 'Proteins').click()
  await clickRow(rows(page).first())
  const detail = page.getByTestId('results-detail')
  await expect(detail).toBeVisible()
  const before = (await detail.boundingBox())!.width
  const splitter = page.getByTestId('splitter')
  await expect(splitter).toHaveAttribute('aria-orientation', 'vertical')
  await splitter.focus()
  // (the detail is after the line: towards it makes it narrower)
  await page.keyboard.press('Shift+ArrowRight')
  await expect.poll(async () => (await detail.boundingBox())!.width).toBe(before - 64)
  // by the pointer
  const handle = (await splitter.boundingBox())!
  await page.mouse.move(handle.x + handle.width / 2, handle.y + 200)
  await page.mouse.down()
  await page.mouse.move(handle.x + handle.width / 2 - 100, handle.y + 200, { steps: 4 })
  await page.mouse.up()
  await expect.poll(async () => (await detail.boundingBox())!.width).toBe(before - 64 + 100)

  // kept for the next start of the app
  await page.reload()
  await expect(page.getByTestId('results-tabs')).toBeVisible()
  await tab(page, 'Proteins').click()
  await clickRow(rows(page).first())
  await expect.poll(async () => (await detail.boundingBox())!.width).toBe(before + 36)

  // the PSM table and the PSM view: one above the other
  await tab(page, 'PSMs').click()
  await clickRow(rows(page).first())
  await expect(page.getByTestId('psm-view')).toBeVisible()
  await expect(page.getByTestId('splitter')).toHaveAttribute('aria-orientation', 'horizontal')
  const shown = await rows(page).count()
  await page.getByTestId('splitter').focus()
  await page.keyboard.press('Shift+ArrowUp')
  await expect.poll(() => rows(page).count()).toBeLessThan(shown)
  expect(await scrolling(page)).toEqual({ view: [], page: false })
})

test('the keys of the PSM view work from the PSM table, where the focus is: A and R mark the PSM, ← → and ↑ ↓ step', async ({ page }) => {
  await tab(page, 'PSMs').click()
  await clickRow(rows(page).first())
  const view = page.getByTestId('psm-view')
  await expect(view.getByTestId('spectrum')).toBeVisible()
  const inTable = () => page.evaluate(() => document.activeElement?.closest('[data-testid="results-table-area"]') !== null)
  expect(await inTable()).toBe(true)

  const accept = view.getByRole('button', { name: /Accept/ })
  const reject = view.getByRole('button', { name: /Reject/ })
  await page.keyboard.press('a')
  await expect(accept).toHaveAttribute('aria-pressed', 'true')
  await page.keyboard.press('r')
  await expect(reject).toHaveAttribute('aria-pressed', 'true')
  await expect(accept).toHaveAttribute('aria-pressed', 'false')
  // the mark is the PSM's: the next one has none, and it is still there on coming back
  await page.keyboard.press('ArrowRight')
  await expect(position(page)).toHaveText('2 of 132')
  await expect(reject).toHaveAttribute('aria-pressed', 'false')
  await page.keyboard.press('ArrowDown')
  await expect(position(page)).toHaveText('3 of 132')
  await page.keyboard.press('ArrowLeft')
  await page.keyboard.press('ArrowUp')
  await expect(position(page)).toHaveText('1 of 132')
  await expect(reject).toHaveAttribute('aria-pressed', 'true')
  await page.keyboard.press('r')
  await expect(reject).toHaveAttribute('aria-pressed', 'false')
  expect(await inTable()).toBe(true)

  // M: the Measure tool; Esc leaves it, the next Esc closes the view (the tabs and the table stay)
  await page.keyboard.press('m')
  await expect(view.getByTestId('tool-measure')).toHaveAttribute('aria-pressed', 'true')
  await page.keyboard.press('Escape')
  await expect(view.getByTestId('tool-measure')).toHaveAttribute('aria-pressed', 'false')
  await expect(view).toBeVisible()
  await page.keyboard.press('Escape')
  await expect(view).toBeHidden()
  await expect(tab(page, 'PSMs')).toHaveAttribute('aria-selected', 'true')
  expect(await inTable()).toBe(true)
})

test('Next and Prev of the PSM view keep the row of the open PSM in sight in the table above', async ({ page }) => {
  await tab(page, 'PSMs').click()
  await clickRow(rows(page).first())
  const view = page.getByTestId('psm-view')
  await expect(view).toBeVisible()
  const open = page.getByTestId('results-table-area').locator('tbody tr[aria-selected="true"]')
  for (let i = 2; i <= 9; i++) {
    await view.getByRole('button', { name: 'Next PSM' }).click()
    await expect(position(page)).toHaveText(`${i} of 132`)
    await expect(open).toBeInViewport({ ratio: 1 })
  }
  // the table's keys go on from that row
  await page.getByTestId('results-table-area').getByRole('grid').focus()
  await page.keyboard.press('ArrowDown')
  await expect(position(page)).toHaveText('10 of 132')
  await expect(open).toBeInViewport({ ratio: 1 })
})

test('Enter on a row whose detail was closed opens the detail again; on the open row it goes on to the level below', async ({ page }) => {
  await tab(page, 'Proteins').click()
  await clickRow(rows(page).nth(1))
  const panel = page.getByTestId('protein-panel')
  await expect(panel).toBeVisible()
  const accession = await panel.getByTestId('panel-accession').innerText()
  // closed: its row stays selected, and Enter is "open it again", not "its peptides"
  await page.keyboard.press('Escape')
  await expect(panel).toBeHidden()
  await page.keyboard.press('Enter')
  await expect(panel.getByTestId('panel-accession')).toHaveText(accession)
  await expect(tab(page, 'Proteins')).toHaveAttribute('aria-selected', 'true')
  await expect(page.getByTestId('narrowing')).toHaveCount(0)
  // open: Enter goes on
  await page.keyboard.press('Enter')
  await expect(tab(page, 'Peptides')).toHaveAttribute('aria-selected', 'true')
  await expect(page.getByTestId('chip-protein')).toBeVisible()
})

test('Backspace goes up from the row the keys are on, also when no detail is open', async ({ page }) => {
  await tab(page, 'Peptides').click()
  // (the keys in the table, no peptide opened: the second row is the active one)
  await page.getByTestId('results-table-area').getByRole('grid').focus()
  await page.keyboard.press('ArrowDown')
  const second = rows(page).nth(1)
  const accession = (await second.locator('td[data-column="proteins"]').innerText()).trim().split(/[\s+]/)[0]
  await page.keyboard.press('Backspace')
  await expect(tab(page, 'Proteins')).toHaveAttribute('aria-selected', 'true')
  await expect(page.getByTestId('protein-panel').getByTestId('panel-accession')).toHaveText(accession)
})

test('the exported spectrum does not depend on the window: the same file from a low window, with its mass errors', async ({ page }) => {
  const exported = async (size: { width: number; height: number }) => {
    await page.setViewportSize(size)
    const view = page.getByTestId('psm-view')
    await expect(view.getByTestId('spectrum')).toBeVisible()
    const [download] = await Promise.all([page.waitForEvent('download'), view.getByRole('button', { name: 'Export spectrum (SVG)' }).click()])
    return readFileSync((await download.path())!, 'utf8')
  }
  await page.setViewportSize({ width: 1440, height: 900 })
  await tab(page, 'PSMs').click()
  await clickRow(rows(page).first())
  const high = await exported({ width: 1440, height: 900 })
  expect(high).toContain('Fragment mass error')
  // a window too low for the error strip: on screen it gives way, in the file it is there
  const low = await exported({ width: 1440, height: 640 })
  await expect(page.getByTestId('psm-view').getByTestId('error-dot')).toHaveCount(0)
  expect(low).toBe(high)
})
