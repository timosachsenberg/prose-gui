// Lists the Rust shim and the page must agree on (review 2026-10-29): read from the Rust sources, so that a change on
// one side fails here instead of drifting.
import { readFileSync } from 'node:fs'
import { join } from 'node:path'
import { expect, test } from 'vitest'
import { LEFT_OUT } from './history/history'
import { isSpectrum } from './search/files'

const rust = (file: string) => readFileSync(join(import.meta.dirname, '../../src-tauri/src', file), 'utf8')
const strings = (line: string) => [...line.matchAll(/"([^"]+)"/g)].map((m) => m[1])
/** The text of the Rust array constant `name` (up to its closing `];`, over the lines rustfmt breaks it into). */
const constant = (file: string, name: string) => {
  const text = rust(file)
  const at = text.indexOf(`pub const ${name}`)
  return at < 0 ? '' : text.slice(at, text.indexOf('];', at))
}

test('the files a results archive leaves out are the same in Rust and TypeScript', () => {
  const line = constant('archive.rs', 'LEFT_OUT')
  expect(new Set(strings(line))).toEqual(LEFT_OUT)
})

test('the spectrum suffixes are the same in Rust and TypeScript', () => {
  const line = constant('args.rs', 'SPECTRUM_SUFFIXES')
  const suffixes = strings(line)
  for (const s of suffixes) expect(isSpectrum(`run${s}`), s).toBe(true)
  for (const other of ['.wiff', '.mgf', '.mzXML']) expect(isSpectrum(`run${other}`), other).toBe(false)
})
