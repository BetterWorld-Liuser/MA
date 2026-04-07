import { ref } from 'vue';
import type { UnlistenFn } from '@tauri-apps/api/event';
import { getCurrentWindow } from '@tauri-apps/api/window';

export function useWindowControls() {
  const appWindow = getCurrentWindow();
  const isMaximized = ref(false);
  let unlistenWindowResize: UnlistenFn | null = null;

  async function initializeWindowState() {
    isMaximized.value = await appWindow.isMaximized();
    unlistenWindowResize = await appWindow.onResized(async () => {
      isMaximized.value = await appWindow.isMaximized();
    });
  }

  function disposeWindowState() {
    if (unlistenWindowResize) {
      unlistenWindowResize();
      unlistenWindowResize = null;
    }
  }

  async function minimizeWindow() {
    await appWindow.minimize();
  }

  async function toggleMaximize() {
    await appWindow.toggleMaximize();
    isMaximized.value = await appWindow.isMaximized();
  }

  async function closeWindow() {
    await appWindow.close();
  }

  return {
    isMaximized,
    initializeWindowState,
    disposeWindowState,
    minimizeWindow,
    toggleMaximize,
    closeWindow,
  };
}
