// The methods section of a search, for a publication: what was searched, how, and how the results were filtered, in
// the past tense and with numbered references, to copy into a manuscript (Overview › Methods section, Export ›
// Methods section). The settings are those ProSE recorded in its provenance view (what it actually used), else
// those the search was started with on ProSE's defaults. It states only what ProSE applied: a Percolator run or a
// PSM FDR that a file skipped, a protein FDR that was off and the missing peptide-level FDR are said as such. No
// local paths. What the app cannot know (the database release, the acquisition) is left to the author: `notes`.

import { percentNumber } from '$lib/format/fdr'
import { baseName } from './model'
import { psmFdrSkipped } from './overview'
import { qpxParameters } from './summary'
import type { ProSEMultiSummary } from './types'
import type { AcceptedCheck } from '../search/checks'
import { ORGANISMS } from '../settings/organisms'

export type MethodsInput = {
  summary: ProSEMultiSummary
  settings: { database: string; contaminants: boolean; fixedMods: string[]; parameters: Record<string, unknown>; rescore: boolean; peptdeep: boolean }
  /** ProSE's parameter defaults (the parameter table of the app's ProSE), for what the provenance does not record. */
  defaults: Record<string, unknown>
  /** ProSE's parameters as its provenance view records them (`SearchResult.provenance`). */
  provenance?: Record<string, string>
  /** The version of ProSE that searched; '' if unknown. */
  version: string
  /** The app's version. */
  appVersion: string
  /** The searched files' names (the design's rows). */
  files: string[]
  /** The instrument of each searched mzML (NT=..;AC=..), by file name. */
  instruments?: Record<string, string>
  /** The cRAP proteins the app appends with contaminants. */
  contaminantProteins: number
  /** The red flags New search was overridden for (`OpenResult.accepted`). */
  accepted?: AcceptedCheck[]
}

/** The methods section: its paragraphs, the references they cite ([n]), and what the author should check or add. */
export type MethodsSection = { paragraphs: string[]; references: string[]; notes: string[] }

/** The works the text cites, in the form most journals' styles start from. */
const REFERENCES = {
  openms:
    'Pfeuffer J, Bielow C, et al. OpenMS 3 enables reproducible analysis of large-scale mass spectrometry data. Nat Methods. 2024;21:365–367. doi:10.1038/s41592-024-02197-7',
  percolator:
    'Käll L, Canterbury JD, Weston J, Noble WS, MacCoss MJ. Semi-supervised learning for peptide identification from shotgun proteomics datasets. Nat Methods. 2007;4:923–925. doi:10.1038/nmeth1113',
  peptdeep:
    'Zeng W-F, Zhou X-X, Willems S, et al. AlphaPeptDeep: a modular deep learning framework to predict peptide properties for proteomics. Nat Commun. 2022;13:7238. doi:10.1038/s41467-022-34904-3',
  targetDecoy:
    'Elias JE, Gygi SP. Target-decoy search strategy for increased confidence in large-scale protein identifications by mass spectrometry. Nat Methods. 2007;4:207–214. doi:10.1038/nmeth1019',
  pickedProtein:
    'Savitski MM, Wilhelm M, Hahne H, Kuster B, Bantscheff M. A scalable approach for protein false discovery rate estimation in large proteomic data sets. Mol Cell Proteomics. 2015;14:2394–2404. doi:10.1074/mcp.M114.046995',
} as const

/**
 * The red flags the text names when the search confirmed them (ProSE reports the same about the files): a flag the
 * search did not confirm was a false alarm and is left out.
 */
const CONFIRMED: Record<string, { held: (s: ProSEMultiSummary) => boolean; text: string }> = {
  'profile-ms2': {
    held: (s) => s.files.some((f) => (f.profileSpectra ?? 0) > 0),
    text: 'Some MS2 spectra were recorded in profile mode and were searched as they were, without centroiding.',
  },
  'no-ms2': {
    held: (s) => s.files.some((f) => f.ms2Spectra === 0),
    text: 'Some input files contained no MS2 spectra.',
  },
}

const count = (n: number) => n.toLocaleString('en')
const WORDS = ['No', 'One', 'Two', 'Three', 'Four', 'Five', 'Six', 'Seven', 'Eight', 'Nine', 'Ten']
/** A count that starts a sentence: spelled out up to ten ("Two runs"), else "A total of 24 runs". */
const leading = (n: number, noun: string) => (n <= 10 ? `${WORDS[n]} ${noun}` : `A total of ${count(n)} ${noun}`)
/** "the run", "both runs", "all 5 runs", "2 of 5 runs": `n` of the `files` runs. */
const ofRuns = (n: number, files: number) => (n < files ? `${n} of ${files} runs` : files === 1 ? 'the run' : files === 2 ? 'both runs' : `all ${files} runs`)
const percent = (q: number) => `${percentNumber(q)}%`
/** "a", "a and b", "a, b and c". */
const and = (items: string[]) => (items.length <= 1 ? (items[0] ?? '') : `${items.slice(0, -1).join(', ')} and ${items.at(-1)}`)
const plural = (n: number, one: string, many: string) => `${n === 1 ? one : many}`
const flag = (v: unknown) => v === true || v === 'true'
const numberOf = (v: unknown, fallback: number) => (typeof v === 'number' ? v : typeof v === 'string' && v !== '' && Number.isFinite(Number(v)) ? Number(v) : fallback)
const listOf = (v: unknown): string[] => (Array.isArray(v) ? v.map(String).filter(Boolean) : typeof v === 'string' && v !== '' ? v.split(/,\s*/) : [])
/** A tolerance value as written: 10, 0.02 (no trailing zeros). */
const value = (n: number) => String(Number(n.toPrecision(6)))
/** A signed whole number with a true minus sign: −1, 0, +1. */
const signed = (n: number) => (n > 0 ? `+${n}` : n < 0 ? `−${-n}` : '0')

/** The input formats of the searched files, as a methods section names them. */
function formatOf(name: string): string {
  const n = name.toLowerCase()
  if (n.endsWith('.raw')) return 'Thermo .raw'
  if (n.endsWith('.d')) return 'Bruker timsTOF .d'
  if (n.endsWith('.mzpeak')) return 'mzPeak'
  return 'mzML'
}

/** The instrument names of an instruments record (NT=..;AC=..), distinct. */
function instrumentNames(instruments: Record<string, string> | undefined, files: string[]): string[] {
  if (!instruments) return []
  const names = files.map((f) => /NT=([^;]+)/.exec(instruments[f] ?? '')?.[1]?.trim() ?? '')
  // (named only when every file has one: a partial list would read as all of them)
  return names.every(Boolean) ? [...new Set(names)] : []
}

/** The database as a methods section names it; `placeholder`: the author has to add its release. */
function databaseOf(file: string): { text: string; placeholder: boolean } {
  const m = /^(UP\d{9})(_reviewed)?\.fasta$/i.exec(file)
  if (!m) return { text: `the protein database ${file}`, placeholder: false }
  const id = m[1].toUpperCase()
  const organism = ORGANISMS.find((o) => o.id === id)?.name
  const species = organism ? (/\(([^)]+)\)/.exec(organism)?.[1] ?? organism) : null
  const what = [species, m[2] ? 'Swiss-Prot entries only' : 'Swiss-Prot and TrEMBL entries'].filter(Boolean).join('; ')
  return { text: `the UniProt reference proteome ${id} (${what}; release [YYYY_MM])`, placeholder: true }
}

export function methodsSection(input: MethodsInput): MethodsSection {
  const { summary: s, settings, version, accepted = [] } = input
  // the settings: what ProSE recorded, else what the search was started with, else ProSE's defaults
  const recorded = input.provenance ? qpxParameters(new Map(Object.entries(input.provenance))) : {}
  const prov = (key: string) => input.provenance?.[key]
  // (an empty list is recorded as '', which qpxParameters skips: none, not the default)
  if (prov('fixed_modifications') === '') recorded['modifications:fixed'] = []
  if (prov('variable_modifications') === '') recorded['modifications:variable'] = []
  const p: Record<string, unknown> = { ...input.defaults, ...settings.parameters, ...recorded }
  const order: (keyof typeof REFERENCES)[] = []
  const cite = (key: keyof typeof REFERENCES) => {
    if (!order.includes(key)) order.push(key)
    return `[${order.indexOf(key) + 1}]`
  }
  const notes: string[] = []
  const files = s.files.length

  // 1. the search
  const search: string[] = []
  const formats = [...new Set(input.files.map(formatOf))]
  const instruments = instrumentNames(input.instruments, input.files)
  const runs = `${leading(files, files === 1 ? 'LC-MS/MS run' : 'LC-MS/MS runs')} (${and(formats)} ${plural(files, 'file', 'files')}${
    instruments.length > 0 ? `; ${and(instruments)}, as recorded in the ${plural(files, 'file', 'files')}` : ''
  })`
  const db = databaseOf(baseName(settings.database))
  if (db.placeholder) notes.push('Fill in the UniProt release of the proteome (the bracketed part); UniProt shows it in the proteome’s download.')
  else notes.push(`Name the source and version of ${baseName(settings.database)} (e.g. the UniProt release or the genome annotation it was made from).`)
  const tool = version ? `ProSE (OpenMS ${version}) ${cite('openms')}` : `ProSE (OpenMS [version]) ${cite('openms')}`
  if (!version) notes.push('The version of ProSE that searched is not recorded: fill it in.')
  search.push(`${runs} ${files === 1 ? 'was' : 'were'} searched with ${tool} against ${db.text}.`)
  const targets = s.search.targetProteins
  if (settings.contaminants) {
    search.push(
      `The ${count(input.contaminantProteins)} common contaminant sequences of cRAP (thegpm.org/crap) were appended${targets > 0 ? `, giving ${count(targets)} target sequences` : ''}.`,
    )
  } else if (targets > 0) {
    search.push(`The database contained ${count(targets)} target sequences.`)
  }

  // decoys
  const decoyMode = s.search.decoyMode || prov('decoy_mode') || ''
  const specificity = String(p['peptide:enzyme_specificity'] ?? 'full')
  if (decoyMode === 'generated') {
    const keepTermini = String(p['decoy_method'] ?? 'reverse') === 'keep_termini' && specificity === 'full'
    search.push(
      keepTermini
        ? `Decoy sequences were generated by ProSE: within each enzymatic peptide, the terminal residues and every K, R and P were kept in place and the other residues reversed, so that each target peptide had a decoy of the same length and mass ${cite('targetDecoy')}.`
        : `Decoy sequences were generated by ProSE by reversing each enzymatic peptide except its C-terminal residue ${cite('targetDecoy')}.`,
    )
  } else if (decoyMode === 'external') {
    search.push(`The decoy sequences contained in the database${s.decoyString ? ` (marked ${s.decoyString})` : ''} were used ${cite('targetDecoy')}.`)
  } else if (s.search.decoyProteins === 0) {
    search.push('The database was searched without decoy sequences.')
  }

  // digestion
  const enzyme = String(prov('enzyme') ?? s.search.enzyme ?? p['enzyme'] ?? '')
  const missed = numberOf(p['peptide:missed_cleavages'], 0)
  const [minLength, maxLength] = [numberOf(p['peptide:min_size'], 0), numberOf(p['peptide:max_size'], 0)]
  const lengths = minLength > 0 && maxLength > 0 ? `peptides of ${minLength} to ${maxLength} residues were considered` : ''
  if (specificity === 'none') {
    search.push(`The search was non-specific (no enzyme constraint at either peptide terminus)${lengths ? `; ${lengths}` : ''}.`)
  } else {
    const how = specificity === 'semi' ? 'semi-specific cleavage (one enzymatic terminus)' : 'full specificity'
    search.push(`${enzyme} was set as the enzyme with ${how} and up to ${missed} missed ${plural(missed, 'cleavage', 'cleavages')}${lengths ? `; ${lengths}` : ''}.`)
  }
  if (flag(prov('clip_nterm_methionine') ?? p['peptide:clip_nterm_methionine'])) {
    search.push('Protein N-terminal peptides were also searched without their initial methionine.')
  }

  // tolerances and charges
  const unit = String(p['precursor:mass_tolerance_unit'] ?? 'ppm')
  const [lower, upper] = [numberOf(p['precursor:mass_tolerance_lower'], 0), numberOf(p['precursor:mass_tolerance_upper'], 0)]
  const window = lower === upper ? `±${value(lower)} ${unit}` : `−${value(lower)} to +${value(upper)} ${unit}`
  const [isoMin, isoMax] = [numberOf(p['precursor:isotope_error_min'], 0), numberOf(p['precursor:isotope_error_max'], 0)]
  const isotopes = isoMin !== 0 || isoMax !== 0 ? `, with precursor isotope errors from ${signed(isoMin)} to ${signed(isoMax)} (¹³C spacings) considered` : ''
  const fragment = `${value(numberOf(p['fragment:mass_tolerance'], 0))} ${String(p['fragment:mass_tolerance_unit'] ?? 'Da')}`
  if (s.openSearch) {
    search.push(
      `An open search was performed with a precursor mass window of ${window}; the mass shift of each match was reported unlocalized. The fragment mass tolerance was ${fragment}.`,
    )
  } else {
    search.push(`The precursor mass tolerance was ${window}${isotopes}, and the fragment mass tolerance ${fragment}.`)
  }
  const [minCharge, maxCharge] = [numberOf(p['precursor:min_charge'], 0), numberOf(p['precursor:max_charge'], 0)]
  const fragmentCharge = numberOf(p['fragment:max_charge'], 0)
  if (minCharge > 0 && maxCharge > 0) {
    search.push(
      `Precursor charges ${minCharge === maxCharge ? `of ${minCharge}` : `${minCharge} to ${maxCharge}`} were searched${fragmentCharge > 0 ? `, and fragment ions up to charge ${fragmentCharge} were scored` : ''}.`,
    )
  }

  // modifications
  const fixed = 'modifications:fixed' in recorded || 'modifications:fixed' in settings.parameters ? listOf(p['modifications:fixed']) : settings.fixedMods
  const variable = listOf(p['modifications:variable'])
  const maxVariable = numberOf(p['modifications:variable_max_per_peptide'], 0)
  const fixedText = `${and(fixed)} ${plural(fixed.length, 'was set as a fixed modification', 'were set as fixed modifications')}`
  const variableText = `${and(variable)} ${plural(variable.length, 'was searched as a variable modification', 'were searched as variable modifications')}${maxVariable > 0 ? ` (at most ${maxVariable} per peptide)` : ''}`
  if (fixed.length > 0 && variable.length > 0) search.push(`${fixedText}, and ${variableText}.`)
  else if (fixed.length > 0) search.push(`${fixedText}; no variable modifications were searched.`)
  else if (variable.length > 0) search.push(`No fixed modifications were set; ${variableText}.`)
  else search.push('No modifications were searched.')
  if (flag(prov('calibration') ?? p['calibration:enabled'])) {
    // (ProSE narrows the fragment tolerance to the errors it measures, the precursor window only where
    // calibration:precursor_sigma_factor is set, and never beyond the tolerances set; with too few confident matches
    // it keeps them. The m/z values are corrected only with calibration:recalibrate.)
    const recalibrate = String(p['calibration:recalibrate'] ?? 'none')
    const corrected =
      recalibrate === 'offset'
        ? ' and the m/z values were corrected for the median error measured'
        : recalibrate === 'linear'
          ? ' and the m/z values were corrected by a line fitted to the errors measured'
          : ''
    search.push(
      `Before the main search, a calibration pass on a subset of the spectra measured the mass errors: within the tolerances set, the fragment tolerance was narrowed to them where enough confident matches were found${corrected}.`,
    )
  }
  if (flag(p['chimeric:enabled'])) {
    search.push(
      'Co-isolated precursors were also identified: after the peaks explained by the best match of a spectrum were removed, the remaining peaks were searched for each further precursor found in its isolation window in the MS1 survey scan.',
    )
  }

  // 2. rescoring and the FDR
  const filtering: string[] = []
  const rescored = s.files.filter((f) => f.percolator?.applied)
  const extra = listOf(prov('extra_features'))
  const peptdeep = settings.peptdeep && rescored.length > 0 && (extra.length === 0 || extra.some((f) => f.startsWith('ms2_') || f === 'rt_abs_error'))
  if (rescored.length > 0) {
    const nces = [...new Set(rescored.map((f) => f.peptdeepNce).filter((n): n is number => n !== null))]
    const instrument = String(p['peptdeep:instrument'] ?? '')
    const auto = numberOf(p['peptdeep:nce'], -1) < 0
    const model = [instrument ? `MS2 model for ${instrument} instruments` : '', nces.length > 0 ? `NCE ${nces.join(', ')}${auto ? ', chosen from the spectra' : ''}` : ''].filter(Boolean)
    filtering.push(
      `PSMs were rescored with Percolator ${cite('percolator')} as implemented in OpenMS, on ProSE's PSM features` +
        (peptdeep ? ` and the fragment-intensity and retention-time predictions of AlphaPeptDeep ${cite('peptdeep')}${model.length > 0 ? ` (${model.join(', ')})` : ''}` : '') +
        (rescored.length < files ? `, for ${ofRuns(rescored.length, files)}` : '') +
        '.',
    )
    const fallback = rescored.filter((f) => f.percolator?.warning)
    if (fallback.length > 0) {
      filtering.push(
        `For ${ofRuns(fallback.length, files)}, Percolator's check of the learned model on held-out data rejected it, and the PSMs were scored with the best single feature.`,
      )
    }
  }
  const notRescored = s.files.filter((f) => f.percolator && !f.percolator.applied)
  if (notRescored.length > 0) filtering.push(`Percolator was not applied to ${ofRuns(notRescored.length, files)}, whose PSMs kept ProSE's score.`)
  const fdrSkipped = s.psmFdr === 0 ? [] : psmFdrSkipped(s)
  if (s.psmFdr === 0 || s.search.decoyProteins === 0) {
    filtering.push('No false discovery rate was estimated: the PSMs are not FDR-controlled.')
  } else if (fdrSkipped.length === files) {
    filtering.push('ProSE reported the PSM-level FDR filter as not applied (no decoy PSMs), so the PSMs are not FDR-controlled.')
  } else {
    const groups = rescored.length === 0 && String(p['FDR:PSM_groups'] ?? '') === 'scored_charges' ? ', separately for PSMs scored with different numbers of fragment charges' : ''
    const source = rescored.length > 0 ? 'Percolator q-values' : `q-values from target-decoy competition ${cite('targetDecoy')}${groups}`
    const perRun = s.proteinFdrApplied ? '' : ', estimated for each run separately'
    filtering.push(`PSMs were filtered at ${percent(s.psmFdr)} PSM-level FDR (${source}${perRun}).`)
    if (fdrSkipped.length > 0) filtering.push(`For ${ofRuns(fdrSkipped.length, files)}, ProSE reported the PSM FDR as not applied (no decoy PSMs), so their PSMs are not FDR-controlled.`)
    filtering.push('No peptide-level FDR was applied; peptide counts are therefore not FDR-controlled at the peptide level.')
  }
  if (s.proteinFdrApplied) {
    const inference = numberOf(p['FDR:protein_inference_PSM'], 0)
    filtering.push(
      `Proteins were inferred from the pooled PSMs of all runs${inference > s.psmFdr ? ` with q ≤ ${value(inference)}` : ''}, indistinguishable proteins grouped, and protein groups filtered at ${percent(s.proteinFdr)} protein-level FDR with the picked-protein approach ${cite('pickedProtein')}.`,
    )
  } else {
    filtering.push('No protein-level FDR was applied; the reported proteins and protein groups are not FDR-controlled.')
  }
  for (const id of new Set(accepted.map((a) => a.id))) if (CONFIRMED[id]?.held(s)) filtering.push(CONFIRMED[id].text)
  // (also a search run outside the app and opened as a QPX folder: so not "run with")
  filtering.push(`The results were inspected with the ProSE desktop app ${input.appVersion} (github.com/timosachsenberg/prose-gui).`)

  notes.push(
    'The liquid chromatography, the acquisition and the sample preparation are not known to the app: describe them in their own paragraphs.',
    'Settings that ProSE does not record in its results (decoy method, isotope errors, variable modifications per peptide, fragment charges) are given as the search was started with them, on the defaults of the app’s ProSE.',
  )
  return { paragraphs: [search.join(' '), filtering.join(' ')], references: order.map((k) => REFERENCES[k]), notes }
}

/** The section as plain text: the paragraphs, then the numbered references. */
export function methodsPlainText(m: MethodsSection): string {
  const refs = m.references.map((r, i) => `[${i + 1}] ${r}`)
  return [...m.paragraphs, ...(refs.length > 0 ? ['References', ...refs] : [])].join('\n\n')
}
