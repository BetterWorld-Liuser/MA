import { nextTick, ref, watch, type Ref } from 'vue';

type UseFloatingModelMenusOptions = {
  filteredModelItems: Ref<unknown[]>;
  modelSearchQuery: Ref<string>;
  plusMenuOpen: Ref<boolean>;
};

export function useFloatingModelMenus({
  filteredModelItems,
  modelSearchQuery,
  plusMenuOpen,
}: UseFloatingModelMenusOptions) {
  const modelMenuAnchorRef = ref<HTMLElement | null>(null);
  const modelMenuPanelRef = ref<HTMLElement | null>(null);
  const modelSearchRef = ref<HTMLInputElement | null>(null);
  const modelMenuOpen = ref(false);
  const modelSettingsAnchorRef = ref<HTMLElement | null>(null);
  const modelSettingsPanelRef = ref<HTMLElement | null>(null);
  const modelSettingsOpen = ref(false);
  const modelMenuStyle = ref<Record<string, string>>({});
  const modelSettingsStyle = ref<Record<string, string>>({});

  watch([modelMenuOpen, filteredModelItems, modelSearchQuery], async ([open]) => {
    if (!open) {
      return;
    }
    await nextTick();
    syncModelMenuPosition();
  });

  watch(modelSettingsOpen, async (open) => {
    if (!open) {
      return;
    }
    await nextTick();
    syncModelSettingsMenuPosition();
  });

  async function toggleModelMenu(primeModelMenu: () => void) {
    if (!modelMenuOpen.value) {
      primeModelMenu();
      plusMenuOpen.value = false;
      modelMenuOpen.value = true;
      modelSearchQuery.value = '';
      await nextTick();
      syncModelMenuPosition();
      modelSearchRef.value?.focus();
      return;
    }
    closeModelMenu();
  }

  async function toggleModelSettingsMenu(
    primeModelMenu: () => void,
    resetModelSettingsDraft: () => void,
  ) {
    if (!modelSettingsOpen.value) {
      primeModelMenu();
      closeModelMenu();
      resetModelSettingsDraft();
      modelSettingsOpen.value = true;
      await nextTick();
      syncModelSettingsMenuPosition();
      return;
    }
    closeModelSettingsMenu();
  }

  function closeModelMenu() {
    modelSearchQuery.value = '';
    modelMenuOpen.value = false;
  }

  function closeModelSettingsMenu() {
    modelSettingsOpen.value = false;
  }

  function syncModelSettingsMenuPosition() {
    if (!modelSettingsOpen.value) {
      return;
    }

    const anchor = modelSettingsAnchorRef.value;
    if (!anchor) {
      return;
    }

    const rect = anchor.getBoundingClientRect();
    const menuWidth = Math.max(320, rect.width + 260);
    const viewportPadding = 12;
    const left = Math.min(
      Math.max(viewportPadding, rect.right - menuWidth),
      window.innerWidth - menuWidth - viewportPadding,
    );

    modelSettingsStyle.value = {
      position: 'fixed',
      left: `${left}px`,
      bottom: `${Math.max(viewportPadding, window.innerHeight - rect.top + 10)}px`,
      width: `${menuWidth}px`,
    };
  }

  function syncModelMenuPosition() {
    if (!modelMenuOpen.value) {
      return;
    }

    const anchor = modelMenuAnchorRef.value;
    if (!anchor) {
      return;
    }

    const rect = anchor.getBoundingClientRect();
    const menuWidth = Math.max(rect.width, 320);
    const viewportPadding = 12;
    const left = Math.min(
      Math.max(viewportPadding, rect.left),
      window.innerWidth - menuWidth - viewportPadding,
    );
    const maxHeight = Math.min(416, window.innerHeight - 144);

    modelMenuStyle.value = {
      position: 'fixed',
      left: `${left}px`,
      bottom: `${Math.max(viewportPadding, window.innerHeight - rect.top + 10)}px`,
      width: `${menuWidth}px`,
      maxHeight: `${maxHeight}px`,
    };
  }

  function syncFloatingMenus() {
    syncModelMenuPosition();
    syncModelSettingsMenuPosition();
  }

  function handleModelMenuPointerDown(
    event: MouseEvent,
    onCloseSettingsExtras?: () => void,
  ) {
    if (!modelMenuOpen.value && !modelSettingsOpen.value) {
      return;
    }

    const target = event.target as Node | null;
    if (!target) {
      return;
    }

    const clickedAnchor = modelMenuAnchorRef.value?.contains(target);
    const clickedPanel = modelMenuPanelRef.value?.contains(target);
    const clickedSettingsAnchor = modelSettingsAnchorRef.value?.contains(target);
    const clickedSettingsPanel = modelSettingsPanelRef.value?.contains(target);
    if (!clickedAnchor && !clickedPanel) {
      closeModelMenu();
    }
    if (!clickedSettingsAnchor && !clickedSettingsPanel) {
      closeModelSettingsMenu();
      onCloseSettingsExtras?.();
    }
  }

  return {
    modelMenuAnchorRef,
    modelMenuPanelRef,
    modelSearchRef,
    modelMenuOpen,
    modelSettingsAnchorRef,
    modelSettingsPanelRef,
    modelSettingsOpen,
    modelMenuStyle,
    modelSettingsStyle,
    toggleModelMenu,
    toggleModelSettingsMenu,
    closeModelMenu,
    closeModelSettingsMenu,
    syncFloatingMenus,
    handleModelMenuPointerDown,
  };
}
