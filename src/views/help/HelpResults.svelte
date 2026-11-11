<script lang="ts">
  // Help › Reading the results: the Overview and its cards (which appear only when something needs attention), the
  // tables, the spectrum, QC and the export; ids "results-…" and "help-groups" (linked from the protein panel).
  import { openHelp } from '$lib/help-link.svelte'
</script>

<section id="results-overview" aria-labelledby="results-overview-h">
  <h2 id="results-overview-h">Reading the Overview</h2>
  <p>
    At the top of Overview, the key figures say what the search found at its own cutoffs: protein groups, peptides, PSMs (each
    with the FDR that controls it), samples and the median precursor error. The numbers on the tabs Proteins, Peptides and PSMs
    are the same counts (proteins, not protein groups) and stay in sight on every tab. Export, at the top right, is described
    below.
  </p>
  <p>
    Overview shows what was searched and how (Search), and the files (Files): for each its MS2 spectra, target and decoy
    PSMs, whether Percolator rescored it (and if not, why), and its status. Marks under a file name say what limited its search
    (no MS2 spectra, profile spectra, spectra not searched because of their charge); “no FDR” under its PSMs says its PSM FDR was
    not applied. The sections below the files, and the notices in them, say what needs attention; each notice names its
    evidence, offers a re-run where one helps, and links to what to do. A notice that the protein FDR was not applied, and the
    “Why so few identifications?” card, appear at the top, and only when they apply.
  </p>
  <ul class="prose-list">
    <li><b>Why so few identifications?</b> A file identifies a smaller share of its MS2 spectra than a search of its kind usually does, or far fewer than the rest of the batch; the likely causes the search reports, strongest first.</li>
    <li><b>Mass accuracy:</b> the precursor and fragment errors of the passing PSMs per file, with the recommended windows, and notices when the errors are off-centre, cut off by the window, or need much less than it. The precursor notices judge the errors of all files together, from 30 passing PSMs on; the fragment notices judge each file, and only for a ppm window.</li>
    <li><b>Checks of the search:</b> the notices New search showed before the search, and what can go wrong silently (an input without a run, spectra counted twice, decoys not recognised or too few of them, all q-values 0, precursors on an isotope peak, retention times in the wrong unit), and under “What ProSE used” the settings as ProSE recorded them beside those you set.</li>
    <li><b>Sample chemistry:</b> the share of peptides with a cysteine against the share expected from the proteins, missed cleavages and non-specific ends (and, for a TMT or iTRAQ label searched as variable, its share on K): signs that the alkylation, enzyme or label of the search do not match the sample.</li>
  </ul>
  <p>
    Always there, below them: the <b>Methods section</b>, the search described for the methods of a publication, with numbered
    references (“Copy”, or Export › Methods section): the settings as ProSE recorded them, and only the FDR filters it applied. It
    leaves to you what the app cannot know (the database release in brackets, the chromatography and the acquisition). Beside it
    “Copy command line”, ProSE’s command as the app ran it. After it, for an open search, <b>Mass shifts</b>: the frequent mass differences; a shift is a mass
    difference, not an identified modification.
  </p>
  <p>
    No card does not mean the search is right: the cards catch what the search can see. <button type="button" class="jump" onclick={() => openHelp('troubleshooting')}>Troubleshooting</button> says what to do about each.
  </p>
</section>

<section id="results" aria-labelledby="results-h">
  <h2 id="results-h">The tables and views</h2>
  <ul class="prose-list">
    <li id="help-groups"><b>Protein groups:</b> a peptide can occur in several proteins. Proteins that share all their identified peptides cannot be told apart and form one group; the evidence for a group is not unique evidence for each protein or isoform in it.</li>
    <li>
      <b>Proteins, Peptides, PSMs:</b> one tab per level. The tabs and the filter row stay where they are, whatever is open. A click
      on a row opens its detail next to the table: a protein its coverage, peptides and PSMs and a peptide its proteins and PSMs
      (beside the table), a PSM its spectrum (under the table). The arrow keys go through the rows and the detail follows; Prev and
      Next of the detail do the same and say where the row is (“3 of 132”). Drag the line between the table and the detail to give
      one of them more room (double-click it for the default). Esc closes the detail. Accessions are links to their protein.
    </li>
    <li id="results-levels">
      <b>From one level to the next:</b> Enter on the open row, a double click on a row, or the buttons of the detail (“Peptides
      of … ›”, “PSMs of … ›”) go on to the level below, narrowed to that protein or peptide. What a table is narrowed to stands at
      the start of its filter row as a chip (“Protein: …”, “Peptide: …”), and its tab says how many rows that leaves (“4 of 132”).
      The Peptides and the PSMs table share the narrowing. A click on a chip takes it away: the table shows all rows again, and
      what is open stays open, so that Prev and Next go through all PSMs from there. Backspace in a table, or “Up to peptide … ·
      protein …” in the spectrum view, goes up a level.
    </li>
    <li><b>Filters:</b> each table has one filter row: text, a q-value cutoff (it can only tighten the search’s), on Proteins the minimum number of peptides and “found in”, and a menu with “Hide contaminants”, “Show decoys” and filters by range or SDRF value. Contaminants and decoys are hidden by default.</li>
    <li>
      <b>Counts:</b> a protein’s peptide count combines the modified forms of one sequence, while the Peptides table lists each
      form as its own row. A PSM of a peptide shared by several proteins counts for each of them. The per-sample columns count
      PSMs, not protein abundance. Leucine and isoleucine have the same mass, so a search cannot tell them apart.
    </li>
    <li>
      <b>Spectrum:</b> measured peaks above, predicted (PeptDeep) or theoretical ions below, and under them the fragment mass
      errors. Drag across it to zoom, double-click to zoom out. The spectrum takes the height under the PSM table and never
      scrolls; in a low window the mass errors give way to it (make the table lower, or the window higher, to see them).
      “Features · Fragments” shows the PSM’s features and its ion table beside the spectrum; hovering an ion in the table marks
      its peak. The keys of the view (A and R to accept or reject, M and L for the tools, ← → for the previous and next PSM) work
      while the PSM table has the focus.
    </li>
    <li><b>Samples:</b> the design of the search: which file belongs to which sample, fraction and replicate.</li>
    <li><b>QC:</b> the scores, the precursor mass errors, the identifications by q-value cutoff (how many target PSMs and peptides each cutoff up to the search’s FDR keeps) and the PSMs per file. The charts show only PSMs that passed the FDR, so the score chart is not a full target-decoy separation plot.</li>
    <li><b>Modification positions</b> are where the search placed them, not localized with a site score.</li>
    <li>
      <b>Export</b> (top right): CSV or Excel of the table you see; the spectrum shown as SVG; the identifications as idXML for other
      OpenMS tools; the design (design.tsv) for a quantification workflow; a methods section for a publication; or a results archive
      (.prose) for colleagues. A results archive holds the
      results and the search record, the sequences of the proteins in the results and the spectra of its PSMs, not the spectrum
      files, the full searched FASTA or the log: to repeat the search, they need those too. To set the same search up again,
      open it from History (Settings), or save the search setup (File › Save Setup: files and settings, no results; Help › File
      formats).
    </li>
  </ul>
</section>
