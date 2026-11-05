// @vitest-environment happy-dom
// The error strip: a dot for every annotated ion it is given (any ion type, a loss, the precursor), in the colours of
// the spectrum, and its header and description; none given: it says so.
import { flushSync, mount, unmount } from 'svelte'
import { afterEach, describe, expect, it } from 'vitest'
import type { PeakMatch } from '../../results/types'
import SpectrumMirror from './SpectrumMirror.svelte'

const match = (peak: number, ion: string, type: string, theoreticalMz: number, errorPpm: number, loss = ''): PeakMatch =>
  ({ peak, ion, type, ordinal: 1, charge: 1, loss, theoreticalMz, errorPpm }) as PeakMatch

describe('the error strip', () => {
  let shown: Record<string, unknown> | null = null
  afterEach(() => {
    if (shown) unmount(shown)
    shown = null
    document.body.innerHTML = ''
  })

  function strip(matches: PeakMatch[]) {
    const target = document.createElement('div')
    document.body.append(target)
    shown = mount(SpectrumMirror, {
      target,
      props: {
        mz: Float32Array.from([175.119, 300.2, 500.25, 801.4]),
        intensity: Float32Array.from([100, 20, 50, 80]),
        groups: [],
        reference: { intensity: 100, text: 'base peak' },
        bottom: { kind: 'status', label: 'Theoretical', message: '', failed: false },
        title: 'Spectrum',
        description: 'A spectrum.',
        errors: { matches, tolerance: { value: 20, unit: 'ppm' } },
      },
    })
    flushSync()
    return {
      target,
      header: target.querySelector('[data-testid="error-stats"]')?.textContent?.replace(/\s+/g, ' ').replace(/ ?· ?/g, ' · ').trim() ?? '',
      desc: target.querySelector('desc')?.textContent ?? '',
    }
  }

  it('has a dot for each annotated ion, in its colour of the spectrum', () => {
    const { target, header, desc } = strip([
      match(0, 'y1', 'y', 175.1189, 1.5),
      match(1, 'a3', 'a', 300.2003, -1),
      match(2, 'b4-H2O', 'b', 500.2504, -0.8, 'H2O'),
      match(3, '[M+2H]++', 'precursor', 801.4006, -0.7),
    ])
    const dots = Array.from(target.querySelectorAll('[data-testid="error-dot"]'))
    expect(dots.map((d) => [d.getAttribute('data-ion'), d.getAttribute('fill')]).sort()).toEqual([
      ['[M+2H]++', 'var(--ion-other)'],
      ['a3', 'var(--ion-other)'],
      ['b4-H2O', 'var(--ion-b)'],
      ['y1', 'var(--ion-y)'],
    ])
    expect(header).toMatch(/^Fragment mass error of the annotated ions · mean [+−-]?\d+\.\d ppm · SD \d+\.\d ppm · dot size = intensity · tolerance ±20 ppm$/)
    expect(desc).toContain('the mass error in ppm of each of the 4 annotated ions shown is drawn under its peak, against its theoretical m/z')
  })

  it('says so when no annotated ion is shown', () => {
    const { header, desc } = strip([])
    expect(header).toBe('Fragment mass error · no annotated ion shown · tolerance ±20 ppm')
    expect(desc).toContain('no annotated ion is shown, so no mass error is shown')
  })

  it('names the one ion shown', () => {
    expect(strip([match(0, 'y1', 'y', 175.1189, 1.5)]).desc).toContain('the mass error in ppm of the annotated ion shown is drawn')
  })
})

describe('the precursor', () => {
  let shown: Record<string, unknown> | null = null
  afterEach(() => {
    if (shown) unmount(shown)
    shown = null
    document.body.innerHTML = ''
  })

  function spectrum(precursor: { mz: number; charge: number } | null) {
    const target = document.createElement('div')
    document.body.append(target)
    shown = mount(SpectrumMirror, {
      target,
      props: {
        mz: Float32Array.from([175.119, 500]),
        intensity: Float32Array.from([100, 50]),
        groups: [],
        reference: { intensity: 100, text: 'base peak' },
        bottom: { kind: 'status', label: 'Theoretical', message: '', failed: false },
        title: 'Spectrum',
        description: 'A spectrum.',
        precursor,
      },
    })
    flushSync()
    return target
  }

  it('is marked at its m/z with its charge, also beyond the peaks', () => {
    const target = spectrum({ mz: 912.4567, charge: 2 })
    const mark = target.querySelector('[data-testid="precursor-mark"]')
    expect(mark?.getAttribute('data-mz')).toBe('912.4567')
    expect(target.querySelector('[data-testid="precursor-label"]')?.textContent).toBe('precursor 2+')
    // (the m/z axis reaches it)
    const [, hi] = target.querySelector('[data-testid="spectrum"]')!.getAttribute('data-mz-domain')!.split(' ').map(Number)
    expect(hi).toBeGreaterThan(912.4567)
    expect(target.querySelector('desc')?.textContent).toContain('A dashed line marks the precursor at m/z 912.4567 (2+).')
  })

  it('is not marked without a known m/z', () => {
    expect(spectrum(null).querySelector('[data-testid="precursor-mark"]')).toBeNull()
    unmount(shown!)
    shown = null
    expect(spectrum({ mz: NaN, charge: 2 }).querySelector('[data-testid="precursor-mark"]')).toBeNull()
  })
})
