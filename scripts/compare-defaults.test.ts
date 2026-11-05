/// <reference types="node" />
// (branch claude/compare-defaults only, not for main) The counts of a search folder as the app reads them, printed as
// RESULT lines: the workflow compare-defaults.yml copies this file to src/ and runs it with RESULT_DIR (the fixture
// folder of results-fixture-native.mjs), CASE_NAME and, for an entrapment search, ENTRAPMENT_FASTA (the searched FASTA,
// whose foreign proteins' accessions start with ENTRAP_).
import { readFileSync } from 'node:fs'
import { join } from 'node:path'
import { it } from 'vitest'
import { bytesOfRange, FileNotFound, readSearch } from './lib/results/read'

const PSM_FDR = 0.01
const PROTEIN_FDR = 0.01
const ENTRAP = 'ENTRAP_'

/** The fully tryptic (Trypsin/P) peptides of 7 to 40 residues with up to 2 missed cleavages (and those of the protein
 * without its initial Met) of the FASTA's entries whose accession passes `keep`, I as L. */
function digest(fasta: string, keep: (accession: string) => boolean, into: Set<string>) {
  let entries = 0
  for (const entry of fasta.split(/^>/m)) {
    const nl = entry.indexOf('\n')
    if (nl < 0) continue
    if (!keep(entry.slice(0, nl).split(/\s/, 1)[0])) continue
    entries++
    const seq = entry.slice(nl + 1).replace(/\s/g, '').replace(/I/g, 'L')
    const sites = [0]
    for (let i = 0; i < seq.length - 1; i++) if (seq[i] === 'K' || seq[i] === 'R') sites.push(i + 1)
    sites.push(seq.length)
    const add = (from: number, to: number) => {
      if (to - from >= 7 && to - from <= 40) into.add(seq.slice(from, to))
    }
    for (let a = 0; a < sites.length - 1; a++) for (let b = a + 1; b <= Math.min(a + 3, sites.length - 1); b++) add(sites[a], sites[b])
    if (seq[0] === 'M') for (let b = 1; b <= Math.min(3, sites.length - 1); b++) add(1, sites[b])
  }
  return entries
}

const pct = (x: number) => `${(100 * x).toFixed(2)} %`

it('counts', { timeout: 1800000 }, async () => {
  const dir = process.env.RESULT_DIR!
  const name = process.env.CASE_NAME ?? dir
  const r = await readSearch(async (file, range) => {
    try {
      return bytesOfRange(new Uint8Array(readFileSync(join(dir, file))), range)
    } catch {
      throw new FileNotFound(file)
    }
  })
  const p = r.tables.psms
  const pt = r.tables.peptides
  const pr = r.tables.proteins
  const rows = Array.from({ length: p.count }, (_, i) => i)
  const accepted = rows.filter((i) => !p.decoy[i] && p.qValue[i] <= PSM_FDR)
  const decoys = rows.filter((i) => p.decoy[i] && p.qValue[i] <= PSM_FDR).length
  const perFile = r.tables.files.map((_, f) => accepted.filter((i) => p.file[i] === f).length)
  const peptides = new Set(accepted.map((i) => pt.openms[p.peptide[i]]))
  const sequences = new Set(accepted.map((i) => pt.sequence[p.peptide[i]]))
  const proteinRows = Array.from({ length: pr.count }, (_, i) => i).filter((i) => !pr.decoy[i] && pr.qValue[i] <= PROTEIN_FDR)
  const fn = p.featureNames.indexOf('protein_nterm')
  const nterm = fn < 0 ? NaN : accepted.filter((i) => p.features[i * p.featureNames.length + fn] === 1).length
  const acetyl = accepted.filter((i) => pt.openms[p.peptide[i]].startsWith('.(Acetyl)')).length
  const proteinsOf = (k: number) => {
    const out: string[] = []
    for (let j = pt.proteinOffsets[k]; j < pt.proteinOffsets[k + 1]; j++) out.push(pr.accession[pt.protein[j]])
    return out
  }
  const species: Record<string, number> = {}
  for (const i of accepted) {
    const s = /_(HUMAN|YEAST|ECOLI)\b/.exec(proteinsOf(p.peptide[i]).join(' '))?.[1] ?? 'other'
    species[s] = (species[s] ?? 0) + 1
  }
  const summary = r.summary
  const percolator = summary.files.map((f) => (f.percolator?.warning ? 'single feature' : f.percolator?.applied ? 'model' : f.percolator?.skipped))
  console.log(
    `RESULT ${name}: target PSMs at 1 % ${accepted.length} (per run ${perFile.join(' / ')}), decoy PSMs at 1 % ${decoys}, ` +
      `peptidoforms ${peptides.size}, sequences ${sequences.size}, target proteins at 1 % ${proteinRows.length}, ` +
      `protein N-terminal PSMs ${nterm}, N-terminally acetylated PSMs ${acetyl}, by species ${JSON.stringify(species)}, ` +
      `percolator ${JSON.stringify(percolator)}, ProSE ${summary.search.secondsTotal.toFixed(0)} s, ` +
      `decoys ${summary.search.decoyMode}, ${summary.search.targetProteins} target / ${summary.search.decoyProteins} decoy proteins, ` +
      `${summary.search.indexedPeptides} indexed peptides`,
  )

  const fasta = process.env.ENTRAPMENT_FASTA
  if (!fasta) return
  // the combined entrapment estimate of the FDP (Wen et al. 2025): N_E (1 + 1/r) / (N_T + N_E), r the ratio of the
  // entrapment to the target search space (here the peptides of the digest that no target protein has); its lower
  // bound N_E / (N_T + N_E)
  const text = readFileSync(fasta, 'utf8')
  const targets = new Set<string>()
  const entrapment = new Set<string>()
  const targetProteins = digest(text, (a) => !a.startsWith(ENTRAP), targets)
  const entrapmentProteins = digest(text, (a) => a.startsWith(ENTRAP), entrapment)
  let foreign = 0
  for (const s of entrapment) if (!targets.has(s)) foreign++
  const ratio = foreign / targets.size
  const kindOf = (k: number): 'target' | 'entrapment' | 'unknown' => {
    const s = pt.sequence[k].replace(/I/g, 'L')
    if (targets.has(s)) return 'target'
    const accessions = proteinsOf(k)
    if (accessions.length) return accessions.every((a) => a.startsWith(ENTRAP)) ? 'entrapment' : 'target'
    return entrapment.has(s) ? 'entrapment' : 'unknown'
  }
  const estimate = (kinds: string[], r: number) => {
    const t = kinds.filter((k) => k === 'target').length
    const e = kinds.filter((k) => k === 'entrapment').length
    const u = kinds.length - t - e
    return `${t} target, ${e} entrapment${u ? `, ${u} unknown` : ''}: FDP ${pct((e * (1 + 1 / r)) / (t + e))} (lower bound ${pct(e / (t + e))})`
  }
  const psmKinds = accepted.map((i) => kindOf(p.peptide[i]))
  const peptideKinds = [...new Set(accepted.map((i) => p.peptide[i]))].map(kindOf)
  const proteinRatio = entrapmentProteins / targetProteins
  const proteinKinds = proteinRows.map((i) => (pr.accession[i].startsWith(ENTRAP) ? 'entrapment' : 'target'))
  console.log(
    `RESULT ${name} entrapment (r = ${ratio.toFixed(3)}: ${foreign} foreign of ${entrapment.size} entrapment peptides, ${targets.size} target peptides; ` +
      `proteins ${entrapmentProteins} / ${targetProteins}): PSMs ${estimate(psmKinds, ratio)}; peptidoforms ${estimate(peptideKinds, ratio)}; ` +
      `proteins ${estimate(proteinKinds, proteinRatio)}`,
  )
})
