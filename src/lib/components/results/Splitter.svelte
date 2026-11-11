<script lang="ts">
  // The handle between a table and its detail pane: drag it (or use the arrow keys on it) to give one of them more
  // room. A separator in the ARIA sense: it has a value (the size of the pane it sizes, in pixels) between `min` and
  // `max`. `axis` "x": a vertical bar between panes that stand side by side; "y": a horizontal bar between panes
  // one above the other. `invert`: the pane it sizes is the one after it (dragging towards it makes it smaller).
  let {
    axis,
    value,
    min,
    max,
    label,
    invert = false,
    onChange,
    onReset,
  }: {
    axis: 'x' | 'y'
    value: number
    min: number
    max: number
    label: string
    invert?: boolean
    onChange: (value: number) => void
    /** A double click: back to the default size. */
    onReset?: () => void
  } = $props()

  let dragging = $state(false)
  const clamp = (v: number) => Math.round(Math.max(min, Math.min(Math.max(min, max), v)))

  function onpointerdown(event: PointerEvent) {
    if (event.button !== 0) return
    event.preventDefault()
    const handle = event.currentTarget as HTMLElement
    const start = axis === 'x' ? event.clientX : event.clientY
    const from = value
    handle.setPointerCapture(event.pointerId)
    dragging = true
    const move = (e: PointerEvent) => {
      const delta = (axis === 'x' ? e.clientX : e.clientY) - start
      onChange(clamp(from + (invert ? -delta : delta)))
    }
    const end = () => {
      dragging = false
      handle.removeEventListener('pointermove', move)
      handle.removeEventListener('pointerup', end)
      handle.removeEventListener('pointercancel', end)
    }
    handle.addEventListener('pointermove', move)
    handle.addEventListener('pointerup', end)
    handle.addEventListener('pointercancel', end)
  }

  function onkeydown(event: KeyboardEvent) {
    const grow = axis === 'x' ? 'ArrowRight' : 'ArrowDown'
    const shrink = axis === 'x' ? 'ArrowLeft' : 'ArrowUp'
    if (event.key !== grow && event.key !== shrink) return
    event.preventDefault()
    const step = (event.key === grow ? 1 : -1) * (invert ? -1 : 1) * (event.shiftKey ? 64 : 16)
    onChange(clamp(value + step))
  }
</script>

<!-- (a separator that can be moved is a widget: it takes the focus and the keys) -->
<!-- svelte-ignore a11y_no_noninteractive_tabindex, a11y_no_noninteractive_element_interactions -->
<div
  role="separator"
  tabindex="0"
  aria-orientation={axis === 'x' ? 'vertical' : 'horizontal'}
  aria-label={label}
  aria-valuenow={Math.round(value)}
  aria-valuemin={Math.round(min)}
  aria-valuemax={Math.round(Math.max(min, max))}
  title="{label}: drag, or use the arrow keys{onReset ? '; double-click for the default' : ''}"
  data-testid="splitter"
  class="splitter {axis}"
  class:dragging
  {onpointerdown}
  {onkeydown}
  ondblclick={onReset}>
</div>

<style>
  .splitter {
    position: relative;
    flex: none;
    touch-action: none;
    outline: none;
  }
  .splitter.x {
    width: 9px;
    cursor: col-resize;
  }
  .splitter.y {
    height: 9px;
    cursor: row-resize;
  }
  /* the line: the pane's border, which lights up under the pointer, while dragging and with the focus */
  .splitter::after {
    content: '';
    position: absolute;
    background: var(--color-border);
  }
  .splitter.x::after {
    inset: 0 4px;
  }
  .splitter.y::after {
    inset: 4px 0;
  }
  .splitter:hover::after,
  .splitter.dragging::after,
  .splitter:focus-visible::after {
    background: var(--color-ring);
  }
  .splitter.x:hover::after,
  .splitter.x.dragging::after,
  .splitter.x:focus-visible::after {
    inset: 0 3px;
  }
  .splitter.y:hover::after,
  .splitter.y.dragging::after,
  .splitter.y:focus-visible::after {
    inset: 3px 0;
  }
</style>
