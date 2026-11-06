import { beforeEach, describe, expect, test, vi } from 'vitest'
import type { FileFacts } from '../desktop'

// A setup through New search (the shim's commands, History and the router are replaced): the setup
// of the form saved and brought back, and what a setup whose files moved or changed, or whose search is not in
// History, says.

/** The files on "disk": path -> size (a path not here is missing). */
const disk = new Map<string, number>()
const facts = (path: string): FileFacts => {
  const name = path.split('/').pop()!
  const size = disk.get(path)
  return {
    path,
    name,
    size: size ?? 0,
    spectra: name.endsWith('.fasta') ? null : 139,
    proteins: name.endsWith('.fasta') ? 4136 : null,
    residues: name.endsWith('.fasta') ? 1306000 : null,
    error: size === undefined ? 'not found' : null,
    problem: null,
    warnings: [],
  }
}
const saved: { name: string; contents: string; ask: boolean }[] = []
let toOpen: { path: string; text: string } | null = null
vi.mock('../desktop', () => ({
  inspectFiles: vi.fn(async (paths: string[]) => paths.map(facts)),
  inspectSpectra: vi.fn(async (paths: string[]) => paths.map(() => null)),
  errorText: (e: unknown) => String((e as Error)?.message ?? e),
  saveSetupFile: vi.fn(async (name: string, contents: string, ask: boolean) => {
    saved.push({ name, contents, ask })
    return { path: `/setups/${name}`, name }
  }),
  openSetupFile: vi.fn(async () => toOpen),
  useOpenedSetupFile: vi.fn(async () => {}),
  forgetSetupFile: vi.fn(async () => {}),
  // (UniProt: the proteome lands in the app's databases folder, or fails when `uniprotDown`)
  downloadProteome: vi.fn(async (id: string, reviewed: boolean) => {
    if (uniprotDown) throw new Error('UniProt cannot be reached')
    const path = `/app/databases/${id}${reviewed ? '_reviewed' : ''}.fasta`
    disk.set(path, 5000)
    return path
  }),
}))
let uniprotDown = false
const navigate = vi.fn()
vi.mock('../route.svelte', () => ({ navigate: (r: unknown) => navigate(r), router: { route: { view: 'search' }, resultId: null } }))
vi.mock('svelte-sonner', () => ({ toast: { success: vi.fn(), warning: vi.fn(), error: vi.fn() } }))
const historyState = { ready: true, entries: [] as { id: string; name: string; error?: string }[] }
const openFromHistory = vi.fn(async (_id: string) => {})
vi.mock('../history/history.svelte', () => ({ history: historyState, openFromHistory: (id: string) => openFromHistory(id), refreshHistory: vi.fn() }))
vi.mock('../search/session.svelte', () => ({ onSearchEnded: vi.fn(), openResult: { current: null }, searchRun: { status: 'idle' }, runSearch: vi.fn() }))
vi.mock('../results/view.svelte', () => ({ currentResultsView: () => null, resultsViewFor: vi.fn() }))

const { searchForm: form } = await import('../search/form.svelte')
const { toolState } = await import('../search/tool.svelte')
const { captureSetup, hasUnsavedChanges, newSetup, openSetup, setupFile, restoreSetup, saveSetup } = await import('./setup-file.svelte')
const desktop = await import('../desktop')
const { parseSetup, serializeSetup } = await import('./setup')

const settle = () => new Promise((r) => setTimeout(r, 0))

describe('setups in New search', () => {
  beforeEach(async () => {
    disk.clear()
    for (const [p, s] of [['/data/a.mzML', 1000], ['/data/b.mzML', 2000], ['/db/human.fasta', 9000]] as const) disk.set(p, s)
    saved.length = 0
    navigate.mockClear()
    openFromHistory.mockClear()
    historyState.entries = []
    form.files = []
    form.database = null
    form.notices = []
    form.removed = null
    form.choosePreset('orbitrap-hcd-tryptic')
    Object.assign(setupFile, { name: null, searchId: null, busy: false, confirmNew: false, confirmOpen: null })
    Object.assign(toolState, { status: 'ready', info: { version: '3.7.0', path: '/x/ProSE', onnx: true }, error: null })
    await form.addFiles(['/data/a.mzML', '/data/b.mzML', '/db/human.fasta'])
    await settle()
    form.setField(form.files[0].id, 'condition', 'ctrl')
    form.setField(form.files[1].id, 'condition', 'treated')
    form.setField(form.files[1].id, 'replicate', 2)
    form.setParam('precursor:mass_tolerance', 7)
    form.setParam('decoys', 'ignore')
    form.peptdeep = true
    form.threads = 4
    form.advanced = true
  })

  test('the setup is captured with everything to search again', () => {
    const s = captureSetup('exp')
    expect(s.inputs).toEqual([
      { path: '/data/a.mzML', name: 'a.mzML', size: 1000, condition: 'ctrl' },
      { path: '/data/b.mzML', name: 'b.mzML', size: 2000, condition: 'treated', replicate: 2 },
    ])
    expect(s.database).toEqual({ path: '/db/human.fasta', name: 'human.fasta', size: 9000, proteome: null })
    expect(s).toMatchObject({ presetId: 'orbitrap-hcd-tryptic', decoys: 'ignore', peptdeep: true, threads: 4, proseVersion: '3.7.0', search: null })
    expect(s.parameters['precursor:mass_tolerance']).toBe(7)
    expect(s.ui).toMatchObject({ view: 'search', advanced: true, results: null })
  })

  test('saved and opened again: the same form, nothing to report', async () => {
    await saveSetup(false)
    // (no setup yet: Save asks where)
    expect(saved[0]).toMatchObject({ name: 'prose-results.prose-setup.json', ask: true })
    expect(setupFile.name).toBe('prose-results')
    await saveSetup(false)
    expect(saved[1].ask).toBe(false)
    // another setup in the form, then the saved one
    form.files = []
    form.threads = null
    form.peptdeep = false
    form.setParam('precursor:mass_tolerance', 20)
    toOpen = { path: '/setups/prose-results.prose-setup.json', text: saved[0].contents }
    // (those are unsaved changes to the setup: Open asks first, here answered with "Discard changes")
    expect(await openSetup()).toBe(false)
    expect(await openSetup(undefined, true)).toBe(true)
    await settle()
    expect(form.files.map((r) => [r.file.path, r.condition, r.replicate])).toEqual([
      ['/data/a.mzML', 'ctrl', undefined],
      ['/data/b.mzML', 'treated', 2],
    ])
    expect(form.database?.file.path).toBe('/db/human.fasta')
    expect(form.value('precursor:mass_tolerance')).toBe(7)
    expect(form.value('decoys')).toBe('ignore')
    expect(form).toMatchObject({ peptdeep: true, threads: 4, advanced: true, presetId: 'orbitrap-hcd-tryptic' })
    expect(form.notices).toEqual([])
    expect(navigate).toHaveBeenLastCalledWith({ view: 'search' })
  })

  test('a hand-written file without parameters keeps the preset’s, not ProSE’s raw defaults', async () => {
    const setup = parseSetup(serializeSetup(captureSetup('exp')))
    setup.parameters = {}
    await restoreSetup(setup)
    const preset = form.presets.find((p) => p.id === setup.presetId)!
    for (const [name, value] of Object.entries(preset.parameters)) expect(form.values[name]).toEqual(value)
  })

  test('files that are gone or changed size are named, and nothing starts', async () => {
    const setup = parseSetup(serializeSetup(captureSetup('exp')))
    disk.delete('/data/a.mzML')
    disk.set('/data/b.mzML', 2048)
    const notes = await restoreSetup(setup)
    expect(form.files.map((r) => r.file.name)).toEqual(['b.mzML'])
    expect(notes).toHaveLength(2)
    expect(notes[0]).toMatch(/^From “exp”, this file is no longer there: a\.mzML\./)
    expect(notes[1]).toMatch(/^This file has changed since “exp” was saved: b\.mzML \(2,000 bytes then, 2,048 bytes now\)\. Check that it is the same data\.$/)
    expect(form.notices).toEqual(notes)
  })

  test('a missing database, and a search that History has or has not', async () => {
    const setup = parseSetup(serializeSetup(captureSetup('exp')))
    setup.search = { id: '42-1', name: 'exp' }
    setup.ui.view = 'results'
    disk.delete('/db/human.fasta')
    let notes = await restoreSetup(setup)
    expect(notes.some((n) => n.startsWith('Choose the database of “exp” again: human.fasta is no longer at /db/human.fasta'))).toBe(true)
    expect(notes.at(-1)).toBe('The search of “exp” (exp) is not in History on this computer: start it again from New search.')
    expect(openFromHistory).not.toHaveBeenCalled()
    expect(navigate).toHaveBeenLastCalledWith({ view: 'search' })
    // in History: Results opens it
    historyState.entries = [{ id: '42-1', name: 'exp' }]
    disk.set('/db/human.fasta', 9000)
    notes = await restoreSetup(setup)
    expect(openFromHistory).toHaveBeenCalledWith('42-1')
    expect(notes).toEqual([])
    expect(setupFile.searchId).toBe('42-1')
  })

  test('a UniProt proteome no longer on disk is downloaded again and chosen; if that fails, the note stays', async () => {
    disk.set('/app/databases/UP000005640_reviewed.fasta', 5000)
    await form.setDatabase('/app/databases/UP000005640_reviewed.fasta')
    const setup = parseSetup(serializeSetup(captureSetup('exp')))
    expect(setup.database?.proteome).toBe('UP000005640')
    disk.delete('/app/databases/UP000005640_reviewed.fasta')
    vi.mocked(desktop.downloadProteome).mockClear()
    let notes = await restoreSetup(setup)
    expect(desktop.downloadProteome).toHaveBeenCalledWith('UP000005640', true)
    expect(form.database).toMatchObject({ file: { path: '/app/databases/UP000005640_reviewed.fasta' }, error: null })
    expect(notes).toEqual([])
    expect(form.notices).toEqual([])
    expect(hasUnsavedChanges()).toBe(false)
    // UniProt cannot be reached: no database, and the notes say so
    disk.delete('/app/databases/UP000005640_reviewed.fasta')
    uniprotDown = true
    try {
      notes = await restoreSetup(setup)
    } finally {
      uniprotDown = false
    }
    expect(form.database?.error).toBeTruthy()
    expect(notes.some((n) => n.startsWith('Choose the database of “exp” again'))).toBe(true)
    expect(notes).toContain('UP000005640_reviewed.fasta is UniProt proteome UP000005640: download it again under Protein database.')
    const { proteomeDownload } = await import('../search/proteome-download.svelte')
    expect(proteomeDownload).toEqual({ id: null, error: 'UniProt cannot be reached' })
  })

  test('New Search: a new, unsaved setup, cleared as Start over clears it; it asks first about unsaved changes', async () => {
    // an opened setup, unchanged: no question
    await saveSetup(false)
    toOpen = { path: '/setups/exp.prose-setup.json', text: saved[0].contents }
    await openSetup()
    await settle()
    expect(setupFile.name).toBe('exp')
    expect(hasUnsavedChanges()).toBe(false)
    vi.mocked(desktop.forgetSetupFile).mockClear()
    expect(newSetup()).toBe(true)
    expect(form.files).toEqual([])
    expect(form.database).toBeNull()
    expect(form.removed?.files.map((r) => r.file.name)).toEqual(['a.mzML', 'b.mzML'])
    expect(setupFile).toMatchObject({ name: null, searchId: null, confirmNew: false })
    expect(desktop.forgetSetupFile).toHaveBeenCalledTimes(1)
    // Undo of Start over brings the files back; the next Save asks where (no setup any more)
    form.undoRemove()
    expect(form.files).toHaveLength(2)
    await saveSetup(false)
    expect(saved.at(-1)!.ask).toBe(true)

    // opened again, then changed: New Search asks and changes nothing until answered
    await openSetup()
    await settle()
    form.setParam('precursor:mass_tolerance', 15)
    expect(hasUnsavedChanges()).toBe(true)
    expect(newSetup()).toBe(false)
    expect(setupFile.confirmNew).toBe(true)
    expect(setupFile.name).toBe('exp')
    expect(form.files).toHaveLength(2)
    // "Discard changes"
    expect(newSetup(true)).toBe(true)
    expect(form.files).toEqual([])
    expect(setupFile).toMatchObject({ name: null, confirmNew: false })
  })

  test('Open Setup asks first about unsaved changes, and a file it refuses never becomes the setup', async () => {
    await saveSetup(false)
    const good = { path: '/setups/exp.prose-setup.json', text: saved[0].contents }
    toOpen = good
    await openSetup()
    await settle()
    expect(setupFile.name).toBe('exp')
    expect(desktop.useOpenedSetupFile).toHaveBeenCalledTimes(1)

    // changed, then Open Setup (or Open Recent): asked first, nothing opened until answered
    form.setParam('precursor:mass_tolerance', 15)
    vi.mocked(desktop.openSetupFile).mockClear()
    expect(await openSetup(2)).toBe(false)
    expect(setupFile.confirmOpen).toEqual({ recent: 2 })
    expect(desktop.openSetupFile).not.toHaveBeenCalled()
    expect(form.value('precursor:mass_tolerance')).toBe(15)
    // "Discard changes": the recent setup opens
    expect(await openSetup(2, true)).toBe(true)
    expect(desktop.openSetupFile).toHaveBeenCalledWith(2)
    expect(setupFile.confirmOpen).toBeNull()

    // a file that does not read as a setup: an error, and the shim is not told to use it (Save keeps its file)
    vi.mocked(desktop.useOpenedSetupFile).mockClear()
    toOpen = { path: '/setups/broken.prose-setup.json', text: '{"format":"something else"}' }
    await expect(openSetup(undefined, true)).rejects.toThrow()
    expect(desktop.useOpenedSetupFile).not.toHaveBeenCalled()
    expect(setupFile.name).toBe('exp')

    // a setup that reads, but cannot be brought back (the files cannot be inspected): no file is the window's
    // setup any more, and the shim is told so, not to use the new one
    vi.mocked(desktop.forgetSetupFile).mockClear()
    vi.mocked(desktop.inspectFiles).mockRejectedValueOnce(new Error('the shim is gone'))
    toOpen = good
    await expect(openSetup(undefined, true)).rejects.toThrow('the shim is gone')
    expect(desktop.useOpenedSetupFile).not.toHaveBeenCalled()
    expect(desktop.forgetSetupFile).toHaveBeenCalledTimes(1)
    expect(setupFile).toMatchObject({ name: null, searchId: null })
    expect(hasUnsavedChanges()).toBe(false)
  })

  test('the versions of the app and of ProSE are no unsaved changes', async () => {
    Object.assign(toolState, { status: 'loading', info: null })
    await saveSetup(false)
    expect(hasUnsavedChanges()).toBe(false)
    // the tool is loaded after the save (or another ProSE is used): nothing for the user to save
    Object.assign(toolState, { status: 'ready', info: { version: '3.8.0', path: '/x/ProSE', onnx: true } })
    expect(hasUnsavedChanges()).toBe(false)
    form.setParam('precursor:mass_tolerance', 15)
    expect(hasUnsavedChanges()).toBe(true)
  })
})
