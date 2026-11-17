// The help window is rendered once by the shell; anything can open it.

class HelpWindow {
  /** The open page (`topic` or `topic#anchor`), null when closed. */
  page = $state<string | null>(null);

  open(page = 'getting-started') {
    this.page = page;
  }

  close() {
    this.page = null;
  }
}

export const help = new HelpWindow();
