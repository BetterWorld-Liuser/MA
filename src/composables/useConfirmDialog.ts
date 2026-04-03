import { ref } from 'vue';

type ConfirmDialogOptions = {
  title: string;
  description: string;
  body: string;
  confirmLabel: string;
  action: () => Promise<void>;
};

export function useConfirmDialog() {
  const confirmDialogOpen = ref(false);
  const confirmDialogTitle = ref('');
  const confirmDialogDescription = ref('');
  const confirmDialogBody = ref('');
  const confirmDialogLabel = ref('删除');
  const confirmDialogAction = ref<(() => Promise<void>) | null>(null);

  function openConfirmDialog(input: ConfirmDialogOptions) {
    confirmDialogTitle.value = input.title;
    confirmDialogDescription.value = input.description;
    confirmDialogBody.value = input.body;
    confirmDialogLabel.value = input.confirmLabel;
    confirmDialogAction.value = input.action;
    confirmDialogOpen.value = true;
  }

  function closeConfirmDialog() {
    confirmDialogOpen.value = false;
    confirmDialogTitle.value = '';
    confirmDialogDescription.value = '';
    confirmDialogBody.value = '';
    confirmDialogLabel.value = '删除';
    confirmDialogAction.value = null;
  }

  function handleConfirmDialogOpenChange(open: boolean) {
    confirmDialogOpen.value = open;

    // Radix/shadcn 的 action/cancel 会先驱动弹窗关闭。
    // 这里不能同步清空 action，否则确认按钮自己的 click handler 会丢失真正的操作。
    if (open) {
      return;
    }

    confirmDialogTitle.value = '';
    confirmDialogDescription.value = '';
    confirmDialogBody.value = '';
    confirmDialogLabel.value = '删除';
  }

  async function submitConfirmDialog() {
    const action = confirmDialogAction.value;
    if (!action) {
      return;
    }
    await action();
  }

  return {
    confirmDialogOpen,
    confirmDialogTitle,
    confirmDialogDescription,
    confirmDialogBody,
    confirmDialogLabel,
    openConfirmDialog,
    closeConfirmDialog,
    handleConfirmDialogOpenChange,
    submitConfirmDialog,
  };
}
