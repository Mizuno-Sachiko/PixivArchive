import { execFileSync } from 'node:child_process';
import { existsSync, readFileSync } from 'node:fs';
import { dirname, isAbsolute, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const repositoryRoot = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const files = execFileSync(
  'git',
  ['ls-files', '-z', '--', '*.md'],
  { cwd: repositoryRoot }
)
  .toString()
  .split('\0')
  .filter(Boolean);
const markdownLink = /!?\[[^\]]*\]\((<[^>]+>|[^)\s]+)(?:\s+["'][^)]+["'])?\)/g;
const missing = [];

for (const file of files) {
  const lines = readFileSync(resolve(repositoryRoot, file), 'utf8').split(/\r?\n/);
  let fenced = false;
  for (const [index, line] of lines.entries()) {
    if (/^\s*(```|~~~)/.test(line)) {
      fenced = !fenced;
      continue;
    }
    if (fenced) continue;

    for (const match of line.matchAll(markdownLink)) {
      const target = match[1].replace(/^<|>$/g, '');
      checkTarget(file, index + 1, target);
    }
  }
}

if (missing.length > 0) {
  for (const failure of missing) console.error(failure);
  process.exit(1);
}

function checkTarget(source, line, rawTarget) {
  const withoutFragment = rawTarget.split('#', 1)[0].split('?', 1)[0];
  if (
    !withoutFragment ||
    withoutFragment.startsWith('/') ||
    isAbsolute(withoutFragment) ||
    /^[a-z][a-z0-9+.-]*:/i.test(withoutFragment)
  ) {
    return;
  }

  let target;
  try {
    target = decodeURIComponent(withoutFragment);
  } catch {
    missing.push(`${source}:${line}: invalid encoded document target ${rawTarget}`);
    return;
  }
  const absolute = resolve(repositoryRoot, dirname(source), target);
  if (!existsSync(absolute)) {
    missing.push(`${source}:${line}: missing local document target ${rawTarget}`);
  }
}
