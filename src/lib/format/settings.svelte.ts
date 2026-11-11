// Display settings that persist across sessions.

export type ModMode = 'marked' | 'proforma'

const storageKey = 'prose.modMode'

function read(): ModMode {
  try {
    return localStorage.getItem(storageKey) === 'proforma' ? 'proforma' : 'marked'
  } catch {
    return 'marked' // no localStorage (tests, blocked storage)
  }
}

let modMode = $state<ModMode>(read())

const massErrorsKey = 'prose.massErrors'

function readMassErrors(): boolean {
  try {
    return localStorage.getItem(massErrorsKey) !== 'hidden'
  } catch {
    return true
  }
}

let massErrors = $state<boolean>(readMassErrors())

/** A size in pixels the user set by dragging (null: the default), kept under `key`. */
function sizeSetting(key: string) {
  let value = $state<number | null>(null)
  try {
    const stored = Number(localStorage.getItem(key))
    if (Number.isFinite(stored) && stored > 0) value = stored
  } catch {
    // no localStorage (tests, blocked storage)
  }
  return {
    get: () => value,
    set(next: number | null) {
      value = next
      try {
        if (next === null) localStorage.removeItem(key)
        else localStorage.setItem(key, String(Math.round(next)))
      } catch {
        // the size then lasts until the page closes
      }
    },
  }
}

const psmSideKey = 'prose.psmSide'

function readPsmSide(): boolean | null {
  try {
    const stored = localStorage.getItem(psmSideKey)
    return stored === 'open' ? true : stored === 'closed' ? false : null
  } catch {
    return null
  }
}

let psmSide = $state<boolean | null>(readPsmSide())

const detailWidth = sizeSetting('prose.detailWidth')
const psmTableHeight = sizeSetting('prose.psmTableHeight')

export const displaySettings = {
  /**
   * The PSM view shows the column of the PSM's features and its ion table beside the spectrum (true) or not
   * (false), as the user last chose; null: not chosen (it shows where the view is wide enough for both).
   */
  get psmSide(): boolean | null {
    return psmSide
  },
  set psmSide(value: boolean | null) {
    psmSide = value
    try {
      if (value === null) localStorage.removeItem(psmSideKey)
      else localStorage.setItem(psmSideKey, value ? 'open' : 'closed')
    } catch {
      // the choice then lasts until the page closes
    }
  },
  /** The width of the detail pane beside the Proteins and Peptides tables, as dragged (null: the default). */
  get detailWidth(): number | null {
    return detailWidth.get()
  },
  set detailWidth(value: number | null) {
    detailWidth.set(value)
  },
  /** The height of the PSM table above the PSM view, as dragged (null: the default). */
  get psmTableHeight(): number | null {
    return psmTableHeight.get()
  },
  set psmTableHeight(value: number | null) {
    psmTableHeight.set(value)
  },
  /** How peptides show their modifications: a marked residue (default) or ProForma text. */
  get modMode(): ModMode {
    return modMode
  },
  set modMode(value: ModMode) {
    modMode = value
    try {
      localStorage.setItem(storageKey, value)
    } catch {
      // the setting then lasts until the page closes
    }
  },
  /** The PSM view shows the fragment mass errors under the spectrum (default); false: the strip is collapsed. */
  get massErrors(): boolean {
    return massErrors
  },
  set massErrors(value: boolean) {
    massErrors = value
    try {
      localStorage.setItem(massErrorsKey, value ? 'shown' : 'hidden')
    } catch {
      // the setting then lasts until the page closes
    }
  },
}
