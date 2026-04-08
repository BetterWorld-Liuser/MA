import type { OpenFileItem } from '@/data/mock';

export type OpenFileTreeFileNode = {
  type: 'file';
  key: string;
  scope: string;
  name: string;
  displayPath: string;
  fullPath: string;
  tokenUsage: string;
  tokenCount: number;
  locked: boolean;
  state: NonNullable<OpenFileItem['state']>;
  freshness: OpenFileItem['freshness'];
};

export type OpenFileTreeDirectoryNode = {
  type: 'directory';
  key: string;
  name: string;
  displayPath: string;
  tokenCount: number;
  tokenUsage: string;
  allLocked: boolean;
  children: OpenFileTreeNode[];
};

export type OpenFileTreeNode = OpenFileTreeFileNode | OpenFileTreeDirectoryNode;

type MutableDirectoryNode = {
  type: 'directory';
  key: string;
  name: string;
  segment: string;
  displayPath: string;
  children: Map<string, MutableDirectoryNode | OpenFileTreeFileNode>;
};

const EXTERNAL_GROUP_KEY = '__external__';

export function buildOpenFilesTree(openFiles: OpenFileItem[], workspaceRoot?: string): {
  nodes: OpenFileTreeNode[];
  totalTokens: number;
} {
  const normalizedRoot = normalizePath(workspaceRoot ?? '');
  const internalRoot = createDirectoryNode('', '', '');
  const externalRoot = createDirectoryNode(EXTERNAL_GROUP_KEY, '外部文件', '外部文件');
  let totalTokens = 0;

  for (const file of openFiles) {
    const normalizedPath = normalizePath(file.path);
    const tokenCount = parseTokenUsage(file.tokenUsage);
    totalTokens += tokenCount;

    const placement = resolveDisplayPath(normalizedPath, normalizedRoot);
    const segments = placement.displayPath.split('/').filter(Boolean);
    const fileName = segments[segments.length - 1] || normalizedPath;
    const parentSegments = segments.slice(0, -1);
    const parent = placement.external
      ? ensureDirectoryPath(externalRoot, parentSegments, externalRoot.key)
      : ensureDirectoryPath(internalRoot, parentSegments, '');

    parent.children.set(`file:${file.scope}:${normalizedPath}`, {
      type: 'file',
      key: `file:${file.scope}:${normalizedPath}`,
      scope: file.scope,
      name: fileName,
      displayPath: placement.displayPath,
      fullPath: normalizedPath,
      tokenUsage: file.tokenUsage,
      tokenCount,
      locked: file.locked,
      state: file.state ?? { kind: 'available' },
      freshness: file.freshness,
    });
  }

  const nodes = [
    ...materializeChildren(internalRoot),
    ...(externalRoot.children.size ? [materializeDirectory(externalRoot)] : []),
  ];

  return { nodes, totalTokens };
}

export function isDirectoryNode(node: OpenFileTreeNode): node is OpenFileTreeDirectoryNode {
  return node.type === 'directory';
}

function createDirectoryNode(key: string, name: string, segment: string): MutableDirectoryNode {
  return {
    type: 'directory',
    key,
    name,
    segment,
    displayPath: name,
    children: new Map(),
  };
}

function ensureDirectoryPath(
  root: MutableDirectoryNode,
  segments: string[],
  parentKey: string,
): MutableDirectoryNode {
  let current = root;
  let prefix = parentKey;

  for (const segment of segments) {
    const key = `dir:${prefix ? `${prefix}/` : ''}${segment}`;
    const existing = current.children.get(key);

    if (existing && existing.type === 'directory') {
      current = existing;
      prefix = prefix ? `${prefix}/${segment}` : segment;
      continue;
    }

    const created = createDirectoryNode(key, segment, segment);
    current.children.set(key, created);
    current = created;
    prefix = prefix ? `${prefix}/${segment}` : segment;
  }

  return current;
}

function materializeChildren(root: MutableDirectoryNode): OpenFileTreeNode[] {
  return sortNodes(
    Array.from(root.children.values()).map((child) =>
      child.type === 'directory' ? materializeDirectory(child) : child,
    ),
  );
}

function materializeDirectory(node: MutableDirectoryNode): OpenFileTreeDirectoryNode {
  let working = node;
  const compressedSegments = [node.segment].filter(Boolean);

  while (working.children.size === 1) {
    const onlyChild = Array.from(working.children.values())[0];
    if (onlyChild.type !== 'directory') {
      break;
    }
    compressedSegments.push(onlyChild.segment);
    working = onlyChild;
  }

  const children = materializeChildren(working);
  const tokenCount = children.reduce((sum, child) => sum + child.tokenCount, 0);
  const allLocked = children.every((child) => (child.type === 'file' ? child.locked : child.allLocked));
  const displayName = compressedSegments.length ? `${compressedSegments.join('/')}/` : working.name;

  return {
    type: 'directory',
    key: node.key,
    name: displayName,
    displayPath: displayName,
    tokenCount,
    tokenUsage: formatTokenCount(tokenCount),
    allLocked,
    children,
  };
}

function sortNodes(nodes: OpenFileTreeNode[]) {
  return [...nodes].sort((left, right) => {
    if (left.type !== right.type) {
      return left.type === 'directory' ? -1 : 1;
    }
    return left.name.localeCompare(right.name, undefined, { sensitivity: 'base' });
  });
}

function resolveDisplayPath(path: string, workspaceRoot: string) {
  if (!path) {
    return { external: false, displayPath: '' };
  }

  if (!isAbsolutePath(path)) {
    return { external: false, displayPath: path };
  }

  if (workspaceRoot) {
    const normalizedPath = normalizeComparablePath(path);
    const normalizedRoot = normalizeComparablePath(workspaceRoot);

    if (normalizedPath === normalizedRoot) {
      return { external: false, displayPath: leafName(path) };
    }

    if (normalizedPath.startsWith(`${normalizedRoot}/`)) {
      return { external: false, displayPath: path.slice(workspaceRoot.length + 1) };
    }
  }

  return { external: true, displayPath: path };
}

function isAbsolutePath(path: string) {
  return /^[a-zA-Z]:\//.test(path) || path.startsWith('//');
}

function normalizePath(path: string) {
  return path.replaceAll('\\', '/').replace(/\/+$/, '');
}

function normalizeComparablePath(path: string) {
  return normalizePath(path).toLowerCase();
}

function leafName(path: string) {
  const normalized = normalizePath(path);
  const segments = normalized.split('/');
  return segments[segments.length - 1] || normalized;
}

function parseTokenUsage(value: string) {
  const trimmed = value.trim().toLowerCase();
  if (!trimmed) {
    return 0;
  }

  if (trimmed.endsWith('k')) {
    const numeric = Number.parseFloat(trimmed.slice(0, -1));
    return Number.isFinite(numeric) ? Math.round(numeric * 1000) : 0;
  }

  const numeric = Number.parseInt(trimmed, 10);
  return Number.isFinite(numeric) ? numeric : 0;
}

export function formatTokenCount(tokens: number) {
  if (tokens >= 1000) {
    const rounded = Math.round(tokens / 100) / 10;
    return `${rounded.toFixed(rounded >= 10 ? 0 : 1)}k`;
  }
  return `${tokens}`;
}
