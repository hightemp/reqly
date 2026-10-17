// Desktop window controls. Only available inside Tauri.

export interface WindowController {
  minimize(): Promise<void>;
  toggleMaximize(): Promise<void>;
  close(): Promise<void>;
  isMaximized(): Promise<boolean>;
  /** Calls `listener` after the window is resized (maximize state may change). */
  onResized(listener: () => void): Promise<() => void>;
  /** Shows or hides the OS window frame. */
  setDecorations(native: boolean): Promise<void>;
}

export async function tauriWindow(): Promise<WindowController> {
  const { getCurrentWindow } = await import('@tauri-apps/api/window');
  const win = getCurrentWindow();
  return {
    minimize: () => win.minimize(),
    toggleMaximize: () => win.toggleMaximize(),
    close: () => win.close(),
    isMaximized: () => win.isMaximized(),
    onResized: (listener) => win.onResized(() => listener()),
    setDecorations: (native) => win.setDecorations(native),
  };
}
