import type { ChatImageAttachment, MentionTargetView, SearchSkillView } from '@/data/mock';

export type MentionKind = 'file' | 'directory' | 'skill';

export type MentionItem = {
  path: string;
  kind: MentionKind;
  label: string;
  description?: string;
};

export type ComposerImageAttachment = ChatImageAttachment;

export type ComposerSearchResult = MentionTargetView | SearchSkillView;

export type SearchMode = 'smart' | 'file' | 'directory' | 'skill';

const IMAGE_EXTENSIONS = new Set(['png', 'jpg', 'jpeg', 'gif', 'webp', 'bmp', 'svg']);

export function isImagePath(path: string) {
  const normalized = path.trim().toLowerCase();
  const extension = normalized.split('.').pop();
  return !!extension && IMAGE_EXTENSIONS.has(extension);
}

export function inferImageMediaType(name: string) {
  const extension = name.trim().toLowerCase().split('.').pop();
  switch (extension) {
    case 'png':
      return 'image/png';
    case 'jpg':
    case 'jpeg':
      return 'image/jpeg';
    case 'gif':
      return 'image/gif';
    case 'webp':
      return 'image/webp';
    case 'bmp':
      return 'image/bmp';
    case 'svg':
      return 'image/svg+xml';
    default:
      return 'image/png';
  }
}

export function appendToken(content: string, token: string) {
  const trimmedEnd = content.replace(/\s+$/, '');
  return trimmedEnd ? `${trimmedEnd} ${token} ` : `${token} `;
}
