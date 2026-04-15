import type { OpenFileItem } from '@/data/mock';

export type OpenFilesPanelFileEntry = {
  type: 'file';
  key: string;
  scope: string;
  name: string;
  fullPath: string;
  tokenUsage: string;
  tokenCount: number;
  locked: boolean;
  state: NonNullable<OpenFileItem['state']>;
  freshness: OpenFileItem['freshness'];
};

export type OpenFilesPanelGroupEntry = {
  type: 'group';
  key: string;
  name: string;
  fullPath?: string;
  tokenCount: number;
  tokenUsage: string;
  fileCount: number;
  collapsible: boolean;
  children: OpenFilesPanelEntry[];
};

export type OpenFilesPanelEntry = OpenFilesPanelFileEntry | OpenFilesPanelGroupEntry;

export type OpenFilesPanelSource = {
  key: string;
  name: string;
  fileCount: number;
  tokenCount: number;
  entries: OpenFilesPanelEntry[];
};

type MutableGroup = {
  type: 'group';
  key: string;
  name: string;
  segment: string;
  fullPath?: string;
  children: Map<string, MutableGroup | OpenFilesPanelFileEntry>;
};

type FilePlacement = {
  file: OpenFileItem;
  normalizedPath: string;
  segments: string[];
  fullPath: string;
};

export function buildOpenFilesPanel(openFiles: OpenFileItem[], workspaceRoot?: string): {
  sources: OpenFilesPanelSource[];
  totalTokens: number;
  totalFiles: number;
  lockedCount: number;
} {
  const normalizedRoot = normalizePath(workspaceRoot ?? '');
  let totalTokens = 0;
  let lockedCount = 0;

  const workspaceFiles: FilePlacement[] = [];
  const externalFiles: FilePlacement[] = [];

  for (const file of openFiles) {
    const normalizedPath = normalizePath(file.path);
    const tokenCount = parseTokenUsage(file.tokenUsage);
    totalTokens += tokenCount;

    if (file.locked) {
      lockedCount += 1;
    }

    const placement = resolvePlacement(file, normalizedPath, normalizedRoot);
    if (placement.external) {
      externalFiles.push({
        file,
        normalizedPath,
        segments: placement.segments,
        fullPath: normalizedPath,
      });
      continue;
    }

    workspaceFiles.push({
      file,
      normalizedPath,
      segments: placement.segments,
      fullPath: normalizedPath,
    });
  }

  const sources: OpenFilesPanelSource[] = [];
  const workspaceSource = buildSource('workspace', 'Workspace', workspaceFiles);
  if (workspaceSource) {
    sources.push(workspaceSource);
  }

  const externalSource = buildSource(
    'external',
    'Outside workspace',
    decorateExternalPlacements(externalFiles),
  );
  if (externalSource) {
    sources.push(externalSource);
  }

  return {
    sources,
    totalTokens,
    totalFiles: openFiles.length,
    lockedCount,
  };
}

export function isGroupEntry(entry: OpenFilesPanelEntry): entry is OpenFilesPanelGroupEntry {
  return entry.type === 'group';
}

function buildSource(key: string, name: string, placements: FilePlacement[]): OpenFilesPanelSource | null {
  if (!placements.length) {
    return null;
  }

  const root = createGroup(key, name, '');

  for (const placement of placements) {
    const tokenCount = parseTokenUsage(placement.file.tokenUsage);
    const fileName = placement.segments[placement.segments.length - 1] || placement.normalizedPath;
    const parent = ensureGroupPath(
      root,
      placement.segments.slice(0, -1),
      placement.segments.slice(0, -1),
    );

    parent.children.set(`file:${placement.file.scope}:${placement.normalizedPath}`, {
      type: 'file',
      key: `file:${placement.file.scope}:${placement.normalizedPath}`,
      scope: placement.file.scope,
      name: fileName,
      fullPath: placement.fullPath,
      tokenUsage: placement.file.tokenUsage,
      tokenCount,
      locked: placement.file.locked,
      state: placement.file.state ?? { kind: 'available' },
      freshness: placement.file.freshness,
    });
  }

  const entries = materializeChildren(root);

  return {
    key,
    name,
    entries,
    fileCount: placements.length,
    tokenCount: placements.reduce((sum, placement) => sum + parseTokenUsage(placement.file.tokenUsage), 0),
  };
}

function createGroup(key: string, name: string, segment: string, fullPath?: string): MutableGroup {
  return {
    type: 'group',
    key,
    name,
    segment,
    fullPath,
    children: new Map(),
  };
}

function ensureGroupPath(root: MutableGroup, segments: string[], fullPathSegments: string[]): MutableGroup {
  let current = root;
  const builtSegments: string[] = [];

  for (const [index, segment] of segments.entries()) {
    builtSegments.push(segment);
    const key = `group:${root.key}:${builtSegments.join('/')}`;
    const existing = current.children.get(key);

    if (existing && existing.type === 'group') {
      current = existing;
      continue;
    }

    const created = createGroup(
      key,
      segment,
      segment,
      fullPathSegments.slice(0, index + 1).join('/'),
    );
    current.children.set(key, created);
    current = created;
  }

  return current;
}

function materializeChildren(root: MutableGroup): OpenFilesPanelEntry[] {
  const rawChildren = sortEntries(
    Array.from(root.children.values()).map((child) =>
      child.type === 'group' ? materializeGroup(child, root.children.size > 1) : child,
    ),
  );

  return rawChildren.flatMap((child) => {
    if (Array.isArray(child)) {
      return child;
    }
    return [child];
  });
}

function materializeGroup(node: MutableGroup, keepEvenIfSingleFile: boolean): OpenFilesPanelEntry | OpenFilesPanelEntry[] {
  let working = node;
  const compressedSegments = [node.segment].filter(Boolean);

  // Collapse single-child directory chains so the panel stays readable at a glance.
  while (working.children.size === 1) {
    const onlyChild = Array.from(working.children.values())[0];
    if (onlyChild.type !== 'group') {
      break;
    }
    compressedSegments.push(onlyChild.segment);
    working = onlyChild;
  }

  const children = materializeChildren(working);
  const fileCount = countFiles(children);

  if (!keepEvenIfSingleFile && children.length === 1 && children[0]?.type === 'file') {
    return children;
  }

  return {
    type: 'group',
    key: node.key,
    name: compressedSegments.join('/'),
    fullPath: working.fullPath,
    tokenCount: countTokens(children),
    tokenUsage: formatTokenCount(countTokens(children)),
    fileCount,
    collapsible: fileCount > 1,
    children,
  };
}

function sortEntries(entries: Array<OpenFilesPanelEntry | OpenFilesPanelEntry[]>) {
  return [...entries].sort((left, right) => {
    const leftEntry = Array.isArray(left) ? left[0] : left;
    const rightEntry = Array.isArray(right) ? right[0] : right;

    if (!leftEntry || !rightEntry) {
      return 0;
    }

    if (leftEntry.type !== rightEntry.type) {
      return leftEntry.type === 'group' ? -1 : 1;
    }

    return leftEntry.name.localeCompare(rightEntry.name, undefined, { sensitivity: 'base' });
  });
}

function countFiles(entries: OpenFilesPanelEntry[]) {
  return entries.reduce((sum, entry) => sum + (entry.type === 'file' ? 1 : entry.fileCount), 0);
}

function countTokens(entries: OpenFilesPanelEntry[]) {
  return entries.reduce((sum, entry) => sum + entry.tokenCount, 0);
}

function resolvePlacement(file: OpenFileItem, normalizedPath: string, workspaceRoot: string) {
  if (!normalizedPath) {
    return { external: false, segments: [] as string[] };
  }

  if (!isAbsolutePath(normalizedPath)) {
    return { external: false, segments: normalizedPath.split('/').filter(Boolean) };
  }

  if (workspaceRoot) {
    const normalizedComparablePath = normalizeComparablePath(normalizedPath);
    const normalizedComparableRoot = normalizeComparablePath(workspaceRoot);

    if (normalizedComparablePath.startsWith(`${normalizedComparableRoot}/`)) {
      return {
        external: false,
        segments: normalizedPath.slice(workspaceRoot.length + 1).split('/').filter(Boolean),
      };
    }
  }

  return {
    external: true,
    segments: [leafName(file.path)],
  };
}

function decorateExternalPlacements(placements: FilePlacement[]) {
  if (placements.length <= 1) {
    return placements;
  }

  const segmentLists = placements.map((placement) => splitPathSegments(placement.normalizedPath));
  const commonDirectoryPrefix = longestCommonPrefix(
    segmentLists.map((segments) => segments.slice(0, -1)),
  );
  const sharedGroupLabel = pickSharedExternalGroupLabel(commonDirectoryPrefix);

  return placements.map((placement) => {
    const segments = splitPathSegments(placement.normalizedPath);
    const remaining = commonDirectoryPrefix.length
      ? segments.slice(commonDirectoryPrefix.length)
      : [segments[segments.length - 1] ?? placement.normalizedPath];

    const nextSegments = sharedGroupLabel
      ? [sharedGroupLabel, ...remaining]
      : remaining;

    return {
      ...placement,
      segments: nextSegments.filter(Boolean),
    };
  });
}

function pickSharedExternalGroupLabel(segments: string[]) {
  if (!segments.length) {
    return '';
  }

  const candidate = segments[segments.length - 1] ?? '';
  if (!candidate || /^[a-zA-Z]:$/.test(candidate)) {
    return '';
  }

  return candidate.charAt(0).toUpperCase() + candidate.slice(1);
}

function longestCommonPrefix(segmentLists: string[][]) {
  if (!segmentLists.length) {
    return [];
  }

  const [first, ...rest] = segmentLists;
  const prefix: string[] = [];

  for (const [index, segment] of first.entries()) {
    if (rest.every((segments) => normalizeComparablePath(segments[index] ?? '') === normalizeComparablePath(segment))) {
      prefix.push(segment);
      continue;
    }
    break;
  }

  return prefix;
}

function splitPathSegments(path: string) {
  return normalizePath(path).split('/').filter(Boolean);
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
  const segments = splitPathSegments(path);
  return segments[segments.length - 1] || normalizePath(path);
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
