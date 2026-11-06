<script lang="ts">
  // File › New Search or Open Setup (also Open Recent) while the opened setup has changes that are not saved:
  // asked here first (as the close guard asks). "Save setup" saves them and then goes on; "Discard" goes on without
  // saving; Cancel keeps all.
  import { toast } from 'svelte-sonner'
  import { Button } from '$lib/components/ui/button'
  import * as Dialog from '$lib/components/ui/dialog'
  import { errorText } from '$lib/desktop'
  import { newSetup, openSetup, setupFile, saveSetup } from '$lib/setup/setup-file.svelte'

  let error = $state<string | null>(null)

  const opening = $derived(setupFile.confirmOpen !== null)

  function close() {
    setupFile.confirmNew = false
    setupFile.confirmOpen = null
    error = null
  }

  /** New Search, or the setup to open (which says in a message if it cannot be opened). */
  async function goOn() {
    const open = setupFile.confirmOpen
    if (!open) return void newSetup(true)
    setupFile.confirmOpen = null
    try {
      await openSetup(open.recent, true)
    } catch (e) {
      toast.error('Open Setup failed', { description: errorText(e) })
    }
  }

  async function saveFirst() {
    error = null
    try {
      if ((await saveSetup(false)) !== null) await goOn()
    } catch (e) {
      error = errorText(e)
    }
  }
</script>

<Dialog.Root open={setupFile.confirmNew || opening} onOpenChange={(o) => !o && close()}>
  <Dialog.Content class="gap-4 sm:max-w-sm" data-testid="new-setup-guard">
    <Dialog.Header>
      <Dialog.Title>Save the changes to “{setupFile.name}”?</Dialog.Title>
      <Dialog.Description>
        {opening ? 'Another setup file replaces this setup.' : 'A new search starts from an empty setup.'} The changes you made
        since the setup was opened or saved are lost unless you save them.
      </Dialog.Description>
    </Dialog.Header>
    {#if error}<p class="text-xs text-destructive" role="alert">{error}</p>{/if}
    <Dialog.Footer>
      <Button variant="outline" size="sm" onclick={close}>Cancel</Button>
      <Button variant="outline" size="sm" onclick={goOn} data-testid="new-setup-discard">Discard changes</Button>
      <Button size="sm" onclick={saveFirst}>Save setup</Button>
    </Dialog.Footer>
  </Dialog.Content>
</Dialog.Root>
