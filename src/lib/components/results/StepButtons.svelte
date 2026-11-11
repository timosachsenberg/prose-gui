<script lang="ts">
  // "3 of 53" and the Prev / Next buttons of a detail pane: through the rows of its table as sorted and filtered,
  // which the arrow keys in the table go through as well.
  import { Button } from '$lib/components/ui/button'
  import { formatCount } from '$lib/results/display'
  import type { Neighbours } from '$lib/results/view.svelte'

  let {
    neighbours,
    noun,
    onStep,
  }: {
    neighbours: Neighbours
    /** What the pane shows: "protein", "peptide", "PSM". */
    noun: string
    onStep: (direction: 1 | -1) => void
  } = $props()

  const hidden = $derived(neighbours.position < 0)
</script>

<span
  class="text-xs whitespace-nowrap text-muted-foreground tabular-nums"
  data-testid="detail-position"
  title={hidden ? `The table's filters hide this ${noun}` : `Row ${formatCount(neighbours.position + 1)} of the ${formatCount(neighbours.count)} the table shows`}
  >{hidden ? 'not in the table' : `${formatCount(neighbours.position + 1)} of ${formatCount(neighbours.count)}`}</span>
<Button
  variant="outline"
  size="sm"
  class="h-7 px-2.5 text-xs"
  aria-label="Previous {noun}"
  title={hidden ? `The table's filters hide this ${noun}` : `Previous ${noun} in the table`}
  disabled={neighbours.prev === null}
  onclick={() => onStep(-1)}>‹ Prev</Button>
<Button
  variant="outline"
  size="sm"
  class="h-7 px-2.5 text-xs"
  aria-label="Next {noun}"
  title={hidden ? `The table's filters hide this ${noun}` : `Next ${noun} in the table`}
  disabled={neighbours.next === null}
  onclick={() => onStep(1)}>Next ›</Button>
