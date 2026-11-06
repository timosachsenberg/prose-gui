<script lang="ts">
  // Closing the window while a search runs, or while the setup has changes not saved to its file: the app keeps
  // the window and asks here first. During a search, closing ends ProSE and the search leaves nothing behind;
  // "Keep searching" leaves the window open. (Closing the window again before an answer closes it: the app's
  // rule, src-tauri/src/lib.rs `close_requested`.) Should the search end while the question is open, the app is
  // answered "keep" at once (so that a later close with a search running asks again), and the question says what
  // is left (search/close-guard.ts). Without a search, unsaved changes to the setup are asked about as New Search
  // asks (NewSetupDialog.svelte): save them and close, close without them, or keep the window.
  import { Button } from '$lib/components/ui/button'
  import * as Dialog from '$lib/components/ui/dialog'
  import { answerClose, errorText, onCloseRequested, setSetupUnsaved } from '$lib/desktop'
  import { GUARD_TEXT, guardState } from '$lib/search/close-guard'
  import { searchRun } from '$lib/search/session.svelte'
  import { hasUnsavedChanges, saveSetup, setupFile } from '$lib/setup/setup-file.svelte'
  import { onMount } from 'svelte'

  let open = $state(false)
  /** The question has been answered with a button (the dialog's own close is "keep"). */
  let answered = false
  /** The app has been answered "keep" because the search ended while the question was open. */
  let released = false
  /** The setup had changes not saved to its file when the window was closed. */
  let unsaved = $state(false)
  /** What went wrong with "Save and close". */
  let error = $state<string | null>(null)

  const phase = $derived(guardState(searchRun))
  const text = $derived(GUARD_TEXT[phase])
  /** No search runs (any more): the question is about the setup's changes. */
  const aboutSetup = $derived(unsaved && phase !== 'running')

  // the app asks before closing while the setup has unsaved changes (`set_setup_unsaved`)
  $effect(() => void setSetupUnsaved(hasUnsavedChanges()))

  onMount(() => {
    let stop: (() => void) | null = null
    let gone = false
    void onCloseRequested(() => {
      answered = false
      released = false
      unsaved = hasUnsavedChanges()
      error = null
      open = true
    }).then((unlisten) => {
      if (gone) unlisten()
      else stop = unlisten
    })
    return () => {
      gone = true
      stop?.()
    }
  })

  $effect(() => {
    if (open && phase !== 'running' && !answered && !released) {
      released = true
      void answerClose(false)
    }
  })

  function answer(close: boolean) {
    answered = true
    open = false
    void answerClose(close)
  }

  async function saveAndClose() {
    error = null
    try {
      if ((await saveSetup(false)) !== null) answer(true)
    } catch (e) {
      error = errorText(e)
    }
  }
</script>

<Dialog.Root
  bind:open
  onOpenChange={(next) => {
    if (!next && !answered) void answerClose(false)
  }}>
  <Dialog.Content class="gap-4 sm:max-w-sm" data-testid="close-guard" data-search={phase}>
    {#if aboutSetup}
      <Dialog.Header>
        <Dialog.Title>Save the changes to “{setupFile.name}”?</Dialog.Title>
        <Dialog.Description>
          ProSE closes. The changes you made since the setup was opened or saved are lost unless you save them.
        </Dialog.Description>
      </Dialog.Header>
      {#if error}<p class="text-xs text-destructive" role="alert">{error}</p>{/if}
      <Dialog.Footer>
        <Button variant="outline" size="sm" onclick={() => answer(false)}>Cancel</Button>
        <Button variant="outline" size="sm" onclick={() => answer(true)}>Close without saving</Button>
        <Button size="sm" onclick={saveAndClose}>Save and close</Button>
      </Dialog.Footer>
    {:else}
      <Dialog.Header>
        <Dialog.Title>{text.title}</Dialog.Title>
        <Dialog.Description>
          {text.description}
          {#if unsaved}The changes to the setup “{setupFile.name}” are not saved either.{/if}
        </Dialog.Description>
      </Dialog.Header>
      <Dialog.Footer>
        <Button variant="outline" size="sm" onclick={() => answer(false)}>{text.keep}</Button>
        <Button variant={phase === 'running' ? 'destructive' : 'default'} size="sm" onclick={() => answer(true)}>{text.close}</Button>
      </Dialog.Footer>
    {/if}
  </Dialog.Content>
</Dialog.Root>
