// What New search says about the chosen FASTA (guidance G6): a one-line summary of its entries for the database
// card (organisms, header style, isoforms, contaminants, decoys), and the checks on it: decoys in the database
// while the setting generates more, and decoys that are too few or too many for its targets. A FASTA of several
// organisms is not warned about by itself: xenografts, host-pathogen and spike-in samples need one.

import type { FastaSummary } from '../desktop'
import type { Check } from './checks'

/** "Homo sapiens (20,401 of 20,420 entries) · UniProt headers · 1,532 isoforms · 116 contaminants · no decoys" */
export function fastaLine(s: FastaSummary, proteins: number): string {
  const n = (x: number) => x.toLocaleString('en-US')
  const parts: string[] = []
  if (s.organisms.length > 0) {
    const [name, count] = s.organisms[0]
    parts.push(`${name} (${n(count)} of ${n(proteins)} entries)`)
    if (s.organismCount > 1) parts.push(`${n(s.organismCount)} organisms`)
  } else {
    parts.push('organism not named in the headers')
  }
  if (s.uniprot > 0) parts.push(s.uniprot === proteins ? 'UniProt headers' : `${n(s.uniprot)} UniProt headers`)
  if (s.isoforms > 0) parts.push(`${n(s.isoforms)} ${s.isoforms === 1 ? 'isoform' : 'isoforms'}`)
  if (s.contaminants > 0) parts.push(`${n(s.contaminants)} ${s.contaminants === 1 ? 'contaminant' : 'contaminants'}`)
  parts.push(s.decoyMarker ? `${n(s.decoys)} decoys (${s.decoyMarker})` : 'no decoys')
  return parts.join(' · ')
}

/**
 * The decoys of a database, per target, that a target-decoy FDR can rest on: about one each. Outside this range
 * (a few stray decoys, or decoys added twice) the FDR is not what it says.
 */
export const DECOY_RATIO = { min: 0.5, max: 2 }

/**
 * The checks of the database: its decoys against the setting and against its targets. `database`: its path (what an accepted red flag is kept for) and its entries (null while counting).
 */
export function databaseChecks(
  s: FastaSummary | undefined,
  decoySetting: string,
  setDecoys: (v: string) => void,
  database?: { path: string; proteins: number | null },
): Check[] {
  if (!s) return []
  const out: Check[] = []
  // ponytail: the decoys are counted with the marker DECOY_ (and the common affixes), not a changed decoy_prefix
  const targets = database?.proteins != null ? database.proteins - s.decoys : null
  if (s.decoyMarker && decoySetting === 'auto' && targets !== null) {
    const ratio = targets > 0 ? s.decoys / targets : Infinity
    if (ratio < DECOY_RATIO.min || ratio > DECOY_RATIO.max) {
      const n = (x: number) => x.toLocaleString('en-US')
      out.push({
        // (no red flag: ProSE refuses such a database under 'auto', so searching it anyway would only fail)
        id: 'decoy-ratio',
        level: 'block',
        source: 'database',
        files: [database!.path],
        message:
          `The database has ${n(s.decoys)} ${s.decoys === 1 ? 'decoy' : 'decoys'} (marked ${s.decoyMarker}) for ${n(targets)} targets. ` +
          'A target-decoy FDR needs about one decoy per target, so ProSE does not use these. Let ProSE generate the decoys, or choose a database with one decoy per target.',
        action: { label: 'Generate decoys', run: () => setDecoys('generate') },
      })
    }
  }
  // (Generate is ProSE's default and does the right thing with them: a note, not a warning)
  if (s.decoyMarker && decoySetting === 'generate') {
    out.push({
      id: 'decoys-twice',
      level: 'info',
      source: 'database',
      message: `The database has decoys of its own (${s.decoys.toLocaleString('en-US')}, marked ${s.decoyMarker}). With Generate, the default, ProSE removes them and makes one decoy per target itself. To search with the database’s decoys instead, use them.`,
      action: { label: 'Use the database’s decoys', run: () => setDecoys('auto') },
    })
  }
  return out
}
