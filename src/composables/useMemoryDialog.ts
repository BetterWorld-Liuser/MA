import { ref } from 'vue';
import type { BackendMemoryDetailView } from '@/data/mock';

export function useMemoryDialog() {
  const memoryDialogOpen = ref(false);
  const memoryDialogMode = ref<'create' | 'edit'>('create');
  const memoryDraftId = ref('');
  const memoryDraftType = ref('fact');
  const memoryDraftTopic = ref('');
  const memoryDraftTitle = ref('');
  const memoryDraftContent = ref('');
  const memoryDraftTags = ref('');
  const memoryDraftScope = ref('shared');
  const memoryDraftLevel = ref('project');

  function openCreateMemoryDialog() {
    memoryDialogMode.value = 'create';
    memoryDraftId.value = '';
    memoryDraftType.value = 'fact';
    memoryDraftTopic.value = '';
    memoryDraftTitle.value = '';
    memoryDraftContent.value = '';
    memoryDraftTags.value = '';
    memoryDraftScope.value = 'shared';
    memoryDraftLevel.value = 'project';
    memoryDialogOpen.value = true;
  }

  function openEditMemoryDialog(memory: BackendMemoryDetailView) {
    memoryDialogMode.value = 'edit';
    memoryDraftId.value = memory.id.replace(/^[pg]:/, '');
    memoryDraftType.value = memory.memory_type;
    memoryDraftTopic.value = memory.topic;
    memoryDraftTitle.value = memory.title;
    memoryDraftContent.value = memory.content;
    memoryDraftTags.value = memory.tags.join(' ');
    memoryDraftScope.value = memory.scope;
    memoryDraftLevel.value = memory.level;
    memoryDialogOpen.value = true;
  }

  function closeMemoryDialog() {
    memoryDialogOpen.value = false;
  }

  function handleMemoryDialogOpenChange(open: boolean) {
    if (!open) {
      closeMemoryDialog();
    }
  }

  async function submitMemoryDialog(
    onSave: (payload: {
      id: string;
      memoryType: string;
      topic: string;
      title: string;
      content: string;
      tags: string[];
      scope?: string;
      level?: string;
    }) => Promise<void>,
    focusField?: { id: () => void; content: () => void },
  ) {
    const id = memoryDraftId.value.trim();
    const title = memoryDraftTitle.value.trim();
    const content = memoryDraftContent.value.trim();

    if (!id) {
      focusField?.id();
      return;
    }
    if (!content || !title) {
      focusField?.content();
      return;
    }

    await onSave({
      id,
      memoryType: memoryDraftType.value.trim() || 'fact',
      topic: memoryDraftTopic.value.trim() || 'general',
      title,
      content,
      tags: memoryDraftTags.value.split(/\s+/).filter(Boolean),
      scope: memoryDraftScope.value.trim() || 'shared',
      level: memoryDraftLevel.value.trim() || 'project',
    });
    closeMemoryDialog();
  }

  return {
    memoryDialogOpen,
    memoryDialogMode,
    memoryDraftId,
    memoryDraftType,
    memoryDraftTopic,
    memoryDraftTitle,
    memoryDraftContent,
    memoryDraftTags,
    memoryDraftScope,
    memoryDraftLevel,
    openCreateMemoryDialog,
    openEditMemoryDialog,
    closeMemoryDialog,
    handleMemoryDialogOpenChange,
    submitMemoryDialog,
  };
}
