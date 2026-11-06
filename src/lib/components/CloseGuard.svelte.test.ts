// @vitest-environment happy-dom
// The close guard: asked while a search runs, it says that closing keeps nothing; when the search ends while it
// is open, the app is answered "keep" at once (no question left open there) and the guard says what is left.
// Without a search, it asks about the setup's unsaved changes, and tells the app whenever there are some.
import { flushSync, mount, tick, unmount } from 'svelte'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'

const shim = vi.hoisted(() => ({ ask: null as (() => void) | null, answers: [] as boolean[], unsavedTold: [] as boolean[] }))
vi.mock('$lib/desktop', () => ({
  onCloseRequested: async (handler: () => void) => {
    shim.ask = handler
    return () => (shim.ask = null)
  },
  answerClose: async (close: boolean) => void shim.answers.push(close),
  setSetupUnsaved: async (unsaved: boolean) => void shim.unsavedTold.push(unsaved),
  errorText: (e: unknown) => String((e as Error)?.message ?? e),
}))
// the setup of the window: its name, whether it has unsaved changes, and its Save (-> where; null: cancelled)
const setupState = $state({ name: 'exp' as string | null, changed: false })
let save: (ask: boolean) => Promise<string | null> = async () => '/setups/exp.prose-setup.json'
vi.mock('$lib/setup/setup-file.svelte', () => ({
  setupFile: setupState,
  hasUnsavedChanges: () => setupState.changed,
  saveSetup: (ask: boolean) => save(ask),
}))

const { searchRun } = await import('$lib/search/session.svelte')
const { default: CloseGuard } = await import('./CloseGuard.svelte')

let shown: Record<string, unknown> | null = null
beforeEach(async () => {
  Object.assign(searchRun, { status: 'running', toolEnded: false })
  Object.assign(setupState, { name: 'exp', changed: false })
  save = async () => '/setups/exp.prose-setup.json'
  shim.answers = []
  shim.unsavedTold = []
  shown = mount(CloseGuard, { target: document.body })
  await tick()
})
afterEach(() => {
  if (shown) unmount(shown)
  shown = null
  document.body.innerHTML = ''
})

const guard = () => document.querySelector('[data-testid="close-guard"]')
const buttons = () => [...(guard()?.querySelectorAll('button') ?? [])].map((b) => b.textContent?.trim()).filter((t) => t)
const click = (text: string) => {
  ;[...guard()!.querySelectorAll('button')].find((b) => b.textContent?.trim() === text)!.click()
  flushSync()
}

describe('the close guard', () => {
  it('asks while ProSE runs, and answers with the button', () => {
    shim.ask!()
    flushSync()
    expect(guard()?.textContent).toMatch(/A search is running.*nothing from it is kept/s)
    expect(buttons()).toContain('Keep searching')
    click('Close ProSE')
    expect(shim.answers).toEqual([true])
  })

  it('when the search finishes while it asks, answers "keep" at once and says that the search is saved', () => {
    shim.ask!()
    flushSync()
    // ProSE ends; its results are read
    searchRun.toolEnded = true
    flushSync()
    expect(shim.answers).toEqual([false])
    expect(guard()?.textContent).toMatch(/The search has finished.*saved in History/s)
    expect(guard()?.textContent).not.toMatch(/nothing from it is kept/)
    // done: no second answer
    searchRun.status = 'done'
    flushSync()
    expect(shim.answers).toEqual([false])
    // the user still wants to close
    click('Close ProSE')
    expect(shim.answers).toEqual([false, true])
  })

  it('says so when the search failed or was cancelled meanwhile', () => {
    shim.ask!()
    flushSync()
    searchRun.status = 'cancelled'
    flushSync()
    expect(shim.answers).toEqual([false])
    expect(guard()?.textContent).toMatch(/The search has ended/)
    click('Keep ProSE open')
    expect(shim.answers).toEqual([false, false])
  })

  it('tells the app whenever the setup has unsaved changes, so that closing asks', () => {
    expect(shim.unsavedTold).toEqual([false])
    setupState.changed = true
    flushSync()
    expect(shim.unsavedTold).toEqual([false, true])
    setupState.changed = false
    flushSync()
    expect(shim.unsavedTold).toEqual([false, true, false])
  })

  it('without a search, asks about the setup: Save and close saves first, Cancel keeps the window', async () => {
    Object.assign(searchRun, { status: 'idle', toolEnded: false })
    setupState.changed = true
    flushSync()
    shim.ask!()
    flushSync()
    expect(guard()?.textContent).toMatch(/Save the changes to “exp”\?.*lost unless you save them/s)
    expect(buttons()).toEqual(expect.arrayContaining(['Cancel', 'Close without saving', 'Save and close']))
    // (no search to wait for: the app is answered "keep" at once, as when a search ends meanwhile)
    expect(shim.answers).toEqual([false])
    click('Save and close')
    await tick()
    await tick()
    expect(shim.answers).toEqual([false, true])

    // asked again; the save is cancelled: nothing closes
    save = async () => null
    shim.ask!()
    flushSync()
    click('Save and close')
    await tick()
    await tick()
    expect(shim.answers).toEqual([false, true, false])
    expect(guard()).not.toBeNull()
    // the save fails: said, the window stays
    save = async () => {
      throw new Error('the disk is full')
    }
    click('Save and close')
    await tick()
    await tick()
    expect(guard()?.textContent).toMatch(/the disk is full/)
    expect(shim.answers).toEqual([false, true, false])
    click('Close without saving')
    expect(shim.answers).toEqual([false, true, false, true])
  })

  it('during a search, the search question also says that the setup is not saved', () => {
    setupState.changed = true
    flushSync()
    shim.ask!()
    flushSync()
    expect(guard()?.textContent).toMatch(/A search is running.*The changes to the setup “exp” are not saved either/s)
    click('Keep searching')
    expect(shim.answers).toEqual([false])
  })
})
