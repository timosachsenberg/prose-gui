// No path of a developer's machine in the repository: fails when a tracked file names a folder that exists
// only on someone's machine or on an internal server (a scratch or network file system, a home folder, a
// session's temporary folder). Such paths get in through defaults of scripts, examples in documentation and
// test fixtures recorded from a real run (a tool's output, the provenance view of a QPX folder), and they
// tell the world how internal machines are laid out. CI runs it (.github/workflows/ui.yml):
//
//   node scripts/check-paths.mjs
//
// In text, write a placeholder (<openms>/build, /path/to/folder) or an environment variable (PROSE_TOOLS);
// in a recorded fixture, replace the folder by a neutral one (/data/..., /work/...).
import { execFileSync } from 'node:child_process'
import { readFileSync } from 'node:fs'

/** What to look for, and what the tests' own made-up paths are. */
const PATTERNS = [
  [/\/scratch\//g, 'a scratch folder'],
  [/\/(nfs|beegfs|lustre|gpfs)\//g, 'a network file system'],
  // (the tests' paths: /home/user, /home/ana, /home/me; GitHub's runners: /home/runner)
  [/\/home\/(?!user\/|runner\/|ana\/|me\/)[A-Za-z][A-Za-z0-9._-]*\//g, 'a home folder'],
  // (the tests' paths: /Users/u, C:\Users\u, C:\Users\Ana)
  [/\/Users\/(?!u\/)[A-Za-z][A-Za-z0-9._-]*\//g, 'a home folder'],
  [/[A-Z]:\\Users\\(?!u\\|Ana\\)[A-Za-z][A-Za-z0-9._-]*\\/g, 'a home folder'],
  [/\/tmp\/claude-[0-9]+\//g, "a session's temporary folder"],
]

/** Files that are someone else's, as published. */
const SKIP = new Set([
  'package-lock.json',
  'src-tauri/Cargo.lock',
  // OpenMS's example data: its processing history names the folder it was converted in
  'src-tauri/resources/example/Ecoli_MS2_small.mzML',
])

const files = execFileSync('git', ['ls-files', '-z'], { encoding: 'utf8', maxBuffer: 64 * 1024 * 1024 })
  .split('\0')
  .filter((f) => f && !SKIP.has(f) && !f.startsWith('native/licenses/'))
let found = 0
for (const file of files) {
  let text
  try {
    // (bytes as characters: a path in a binary file, e.g. a Parquet file's statistics, is found too)
    text = readFileSync(file, 'latin1')
  } catch {
    continue // a deleted file not yet committed, a submodule
  }
  for (const [pattern, what] of PATTERNS) {
    for (const m of text.matchAll(pattern)) {
      const line = text.slice(0, m.index).split('\n').length
      const at = text.slice(m.index, m.index + 60).split(/[\s"'`<>)]/)[0]
      console.error(`${file}:${line}: ${what}: ${at}`)
      found += 1
    }
  }
}
if (found > 0) {
  console.error(`\n${found} path(s) of a machine in the repository (scripts/check-paths.mjs says what to write instead).`)
  process.exit(1)
}
console.log(`no path of a developer's machine in ${files.length} files`)
