import { mount } from 'svelte'
import './app.css'
import App from './App.svelte'

// dev only (the browser UI tests, e2e-web/): `?fixture=<name>` opens a fixture result through an in-memory
// History (`&psmFdr=<0..1>`: the PSM FDR it was searched with, default 0.02); import.meta.env.DEV is false in a
// build, so the hook and the fixtures are not in the bundle
const params = import.meta.env.DEV ? new URLSearchParams(location.search) : null
const fixture = params?.get('fixture') ?? null
if (fixture) {
  void import('./lib/dev/fixture').then(async (dev) => {
    const backend = dev.useMemoryHistory()
    mount(App, { target: document.getElementById('app')! })
    const psmFdr = Number(params?.get('psmFdr') ?? NaN)
    await dev.openFixture(backend, fixture, undefined, psmFdr > 0 && psmFdr <= 1 ? psmFdr : undefined)
  })
} else {
  mount(App, { target: document.getElementById('app')! })
}
