import { ref, type Ref } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import type { WorkspaceImageView } from '@/data/mock';
import {
  inferImageMediaType,
  isImagePath,
  type ComposerImageAttachment,
} from './chatComposerShared';

type UseComposerImageAttachmentsOptions = {
  taskId: Ref<number | null | undefined>;
  supportsVision: Ref<boolean>;
  showComposerNotice: (message: string) => void;
};

export function useComposerImageAttachments({
  taskId,
  supportsVision,
  showComposerNotice,
}: UseComposerImageAttachmentsOptions) {
  const imageAttachments = ref<ComposerImageAttachment[]>([]);
  const dragActive = ref(false);

  function removeImageAttachment(id: string) {
    imageAttachments.value = imageAttachments.value.filter((item) => item.id !== id);
  }

  function clearImageAttachments() {
    imageAttachments.value = [];
    dragActive.value = false;
  }

  function addImageAttachment(attachment: ComposerImageAttachment) {
    if (imageAttachments.value.some((item) => item.id === attachment.id)) {
      return;
    }
    imageAttachments.value = [...imageAttachments.value, attachment];
  }

  async function attachImageFiles(files: File[]) {
    if (!files.length) {
      return;
    }
    if (!supportsVision.value) {
      showComposerNotice('当前模型不支持图片输入');
      return;
    }

    const imageFiles = files.filter((file) => file.type.startsWith('image/') || isImagePath(file.name));
    const attachments = await Promise.all(imageFiles.map(fileToImageAttachment));
    attachments.forEach(addImageAttachment);
  }

  async function attachWorkspaceImage(path: string) {
    if (!supportsVision.value) {
      showComposerNotice('当前模型不支持图片输入');
      return false;
    }

    if (!taskId.value) {
      return false;
    }

    const image = await invoke<WorkspaceImageView>('load_workspace_image', {
      input: {
        taskId: taskId.value,
        path,
      },
    });
    addImageAttachment({
      id: `workspace:${image.path}`,
      name: image.name,
      previewUrl: image.dataUrl,
      mediaType: image.mediaType,
      sourcePath: image.path,
    });
    return true;
  }

  async function handleImageFileSelection(event: Event) {
    const target = event.target as HTMLInputElement | null;
    const files = Array.from(target?.files ?? []);
    await attachImageFiles(files);
    if (target) {
      target.value = '';
    }
  }

  async function handlePaste(event: ClipboardEvent) {
    const files = extractImageFiles(event.clipboardData?.items);
    if (!files.length) {
      return;
    }

    event.preventDefault();
    await attachImageFiles(files);
  }

  async function handleDrop(event: DragEvent) {
    dragActive.value = false;
    const files = Array.from(event.dataTransfer?.files ?? []).filter((file) => file.type.startsWith('image/'));
    if (!files.length) {
      return;
    }

    event.preventDefault();
    await attachImageFiles(files);
  }

  function handleDragOver(event: DragEvent) {
    if (!hasImageFile(event.dataTransfer?.items)) {
      dragActive.value = false;
      return;
    }

    event.preventDefault();
    dragActive.value = true;
  }

  function handleDragLeave(event: DragEvent, composerRootRef: Ref<HTMLElement | null>) {
    if (!composerRootRef.value?.contains(event.relatedTarget as Node | null)) {
      dragActive.value = false;
    }
  }

  async function fileToImageAttachment(file: File): Promise<ComposerImageAttachment> {
    const previewUrl = await readFileAsDataUrl(file);
    return {
      id: `upload:${file.name}:${file.size}:${file.lastModified}`,
      name: file.name,
      previewUrl,
      mediaType: file.type || inferImageMediaType(file.name),
    };
  }

  return {
    imageAttachments,
    dragActive,
    removeImageAttachment,
    clearImageAttachments,
    attachWorkspaceImage,
    handleImageFileSelection,
    handlePaste,
    handleDrop,
    handleDragOver,
    handleDragLeave,
  };
}

function extractImageFiles(items?: DataTransferItemList | null) {
  if (!items) {
    return [];
  }
  return Array.from(items)
    .filter((item) => item.kind === 'file' && item.type.startsWith('image/'))
    .map((item) => item.getAsFile())
    .filter((file): file is File => !!file);
}

function hasImageFile(items?: DataTransferItemList | null) {
  if (!items) {
    return false;
  }
  return Array.from(items).some((item) => item.kind === 'file' && item.type.startsWith('image/'));
}

function readFileAsDataUrl(file: File) {
  return new Promise<string>((resolve, reject) => {
    const reader = new FileReader();
    reader.onload = () => {
      if (typeof reader.result === 'string') {
        resolve(reader.result);
        return;
      }
      reject(new Error(`failed to read image file ${file.name}`));
    };
    reader.onerror = () => reject(reader.error ?? new Error(`failed to read image file ${file.name}`));
    reader.readAsDataURL(file);
  });
}
