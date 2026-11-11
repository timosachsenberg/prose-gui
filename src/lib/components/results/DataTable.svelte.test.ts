// @vitest-environment happy-dom
// The table's keyboard: Enter selects the active row; with followFocus (a table whose detail is open) the arrow
// keys, Page Up/Down, Home and End select each row they move to, with the focus staying in the grid. Without it,
// they only move the active row. With `onRowOpen`, Enter on the row that is selected already (and a double click)
// goes on to the level below; Backspace goes back up (`onBack`). New rows keep the selected row the active one.
import { flushSync, mount, unmount } from 'svelte'
import { afterAll, afterEach, beforeAll, describe, expect, it } from 'vitest'
import DataTable, { type Column } from './DataTable.svelte'

const columns: Column[] = [{ id: 'n', header: 'Row', width: 'auto', align: 'left', cell: (i: number) => `row ${i}` }]

// happy-dom lays nothing out: every element would be 0 px high, so the grid would take each row as scrolled out of
// view. A 400 px grid, as in a window.
const clientHeight = Object.getOwnPropertyDescriptor(HTMLElement.prototype, 'clientHeight')
beforeAll(() => Object.defineProperty(HTMLElement.prototype, 'clientHeight', { configurable: true, get: () => 400 }))
afterAll(() => {
  if (clientHeight) Object.defineProperty(HTMLElement.prototype, 'clientHeight', clientHeight)
})

describe('DataTable keys', () => {
  let shown: Record<string, unknown> | null = null
  afterEach(() => {
    if (shown) unmount(shown)
    shown = null
    document.body.innerHTML = ''
  })

  function table(followFocus: boolean) {
    const clicked: number[] = []
    const target = document.createElement('div')
    document.body.append(target)
    shown = mount(DataTable, {
      target,
      props: {
        rows: Uint32Array.from([10, 11, 12, 13]),
        columns,
        sort: { key: 'n', dir: 'asc' as const },
        onSort: () => {},
        onRowClick: (row: number) => clicked.push(row),
        label: 'Test',
        followFocus,
      },
    })
    flushSync()
    const grid = target.querySelector<HTMLElement>('[role="grid"]')!
    const press = (key: string) => {
      grid.dispatchEvent(new KeyboardEvent('keydown', { key, bubbles: true, cancelable: true }))
      flushSync()
    }
    return { clicked, grid, press }
  }

  it('selects each row the keys reach with followFocus', () => {
    const { clicked, press } = table(true)
    press('ArrowDown')
    press('ArrowDown')
    press('ArrowUp')
    press('End')
    press('Home')
    expect(clicked).toEqual([11, 12, 11, 13, 10])
  })

  it('only moves the active row without it; Enter selects', () => {
    const { clicked, press } = table(false)
    press('ArrowDown')
    press('ArrowDown')
    expect(clicked).toEqual([])
    press('Enter')
    expect(clicked).toEqual([12])
  })

  it('does not select rows when the grid is scrolled with the mouse', () => {
    const { clicked, grid } = table(true)
    grid.scrollTop = 50
    grid.dispatchEvent(new Event('scroll'))
    flushSync()
    expect(clicked).toEqual([])
  })

  /**
   * A table with a selected row, the level below (`onRowOpen`) and the way up (`onBack`); `open`: the detail of the
   * selected row is open (`followFocus`), as the level tables say it. The rows the table selects become its
   * selected row, as a level does.
   */
  function levels(selected: number | null, rows = [10, 11, 12, 13], open = true) {
    const clicked: number[] = []
    const opened: number[] = []
    const backs: (number | null)[] = []
    const target = document.createElement('div')
    document.body.append(target)
    const props = $state({
      rows: Uint32Array.from(rows),
      columns,
      sort: { key: 'n', dir: 'asc' as const },
      onSort: () => {},
      onRowClick: (row: number) => {
        clicked.push(row)
        props.selected = row
      },
      onRowOpen: (row: number) => opened.push(row),
      onBack: (row: number | null) => void backs.push(row),
      selected,
      followFocus: open,
      label: 'Test',
    })
    shown = mount(DataTable, { target, props })
    flushSync()
    const grid = target.querySelector<HTMLElement>('[role="grid"]')!
    const press = (key: string) => {
      grid.dispatchEvent(new KeyboardEvent('keydown', { key, bubbles: true, cancelable: true }))
      flushSync()
    }
    const active = () => target.querySelector('tbody tr.active')?.textContent?.trim()
    return { clicked, opened, backs, grid, press, props, target, active }
  }

  it('Enter on the row whose detail is open goes on to the level below; Space only selects', () => {
    const { clicked, opened, press } = levels(11)
    press('Enter') // the open row is the active one: on to its level below
    expect([clicked, opened]).toEqual([[], [11]])
    press('ArrowUp') // (the detail follows the keys: row 10 is open now)
    expect(clicked).toEqual([10])
    press('Enter')
    expect([clicked, opened]).toEqual([[10], [11, 10]])
    press(' ') // Space selects, also the open row
    expect([clicked, opened]).toEqual([[10, 10], [11, 10]])
  })

  it('Enter on a selected row whose detail is closed opens it again, and only then goes on', () => {
    // (the row stays selected when its detail closes: Esc, then Enter, is "open it again", not "its peptides")
    const { clicked, opened, press, props } = levels(11, undefined, false)
    press('Enter')
    expect([clicked, opened]).toEqual([[11], []])
    props.followFocus = true // its detail is open
    flushSync()
    press('Enter')
    expect([clicked, opened]).toEqual([[11], [11]])
  })

  it('a double click goes on to the level below; Backspace goes back up from the active row', () => {
    const { clicked, opened, backs, press, target, props } = levels(null, undefined, false)
    const row = target.querySelectorAll<HTMLElement>('tbody tr[role="row"]')[2]
    row.dispatchEvent(new MouseEvent('dblclick', { bubbles: true }))
    flushSync()
    expect(opened).toEqual([12])
    expect(clicked).toEqual([])
    // the row the keys are on, not one selected before
    press('ArrowDown')
    press('Backspace')
    expect(backs).toEqual([11])
    // a table without rows (a narrowing that leaves none) is left the same way
    props.rows = new Uint32Array(0)
    flushSync()
    press('Backspace')
    expect(backs).toEqual([11, null])
  })

  it('without a level below, Enter always selects and Backspace is not the table\'s', () => {
    const { clicked, grid, press } = table(false)
    press('Enter')
    press('Enter')
    expect(clicked).toEqual([10, 10])
    const event = new KeyboardEvent('keydown', { key: 'Backspace', bubbles: true, cancelable: true })
    grid.dispatchEvent(event)
    expect(event.defaultPrevented).toBe(false)
  })

  it('new rows keep the open row the active one; without it among them, or with no detail open, the first row is', () => {
    const { clicked, opened, press, props, active } = levels(12, [12])
    expect(active()).toBe('row 12')
    // the table shows more rows (a narrowing was taken away): the open row stays the active one
    props.rows = Uint32Array.from([10, 11, 12, 13])
    flushSync()
    expect(active()).toBe('row 12')
    press('Enter')
    expect([clicked, opened]).toEqual([[], [12]])
    press('ArrowDown')
    expect(active()).toBe('row 13')
    // rows without the open one: back to the first
    props.rows = Uint32Array.from([10, 11])
    flushSync()
    expect(active()).toBe('row 10')
    // no detail open (the row is only selected): new rows start at the top, as a sorted or filtered table does
    props.followFocus = false
    props.selected = 12
    flushSync()
    props.rows = Uint32Array.from([10, 11, 12, 13])
    flushSync()
    expect(active()).toBe('row 10')
  })

  it('a selection made outside the table (Prev and Next of the detail) moves the active row with it', () => {
    const { clicked, opened, press, props, active } = levels(10)
    expect(active()).toBe('row 10')
    props.selected = 12
    flushSync()
    expect(active()).toBe('row 12')
    // the keys go on from there
    press('ArrowDown')
    expect(active()).toBe('row 13')
    props.selected = 11
    flushSync()
    expect(active()).toBe('row 11')
    press('Enter')
    expect([clicked, opened]).toEqual([[13], [11]])
    // nothing selected, or a row the table does not show: the active row stays
    props.selected = null
    flushSync()
    expect(active()).toBe('row 11')
    props.selected = 99
    flushSync()
    expect(active()).toBe('row 11')
  })
})
