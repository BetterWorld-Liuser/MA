import fs from 'node:fs';
import path from 'node:path';

const workspaceRoot = process.cwd();
const skippedDirectories = new Set([
  '.git',
  '.ma',
  'dist',
  'node_modules',
  'target',
  'temp-cargo-target',
  '.codex-target',
  '.codex-target-checkUAZ2Dc',
  '.codex-targetipChUt',
  'codex-check-dir',
]);

const shadowPairs = [];

walk(workspaceRoot);

if (shadowPairs.length) {
  console.error('Found shadowing .js files next to TypeScript sources:');
  for (const pair of shadowPairs) {
    console.error(`- ${pair.jsPath} shadows ${pair.sourcePath}`);
  }
  process.exit(1);
}

console.log('No shadowing .js files found next to TypeScript sources.');

function walk(directory) {
  const entries = fs.readdirSync(directory, { withFileTypes: true });

  for (const entry of entries) {
    if (entry.name.startsWith('.') && !entry.name.startsWith('.codex-target')) {
      if (skippedDirectories.has(entry.name)) {
        continue;
      }
    }

    const fullPath = path.join(directory, entry.name);
    if (entry.isDirectory()) {
      if (skippedDirectories.has(entry.name)) {
        continue;
      }
      walk(fullPath);
      continue;
    }

    if (!entry.isFile() || !entry.name.endsWith('.js')) {
      continue;
    }

    const basename = entry.name.slice(0, -3);
    const tsPath = path.join(directory, `${basename}.ts`);
    const tsxPath = path.join(directory, `${basename}.tsx`);

    if (fs.existsSync(tsPath)) {
      shadowPairs.push({
        jsPath: normalizePath(fullPath),
        sourcePath: normalizePath(tsPath),
      });
      continue;
    }

    if (fs.existsSync(tsxPath)) {
      shadowPairs.push({
        jsPath: normalizePath(fullPath),
        sourcePath: normalizePath(tsxPath),
      });
    }
  }
}

function normalizePath(targetPath) {
  return path.relative(workspaceRoot, targetPath).replaceAll('\\', '/');
}
