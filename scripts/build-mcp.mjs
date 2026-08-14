// Builds the MCP server and stages it where Tauri expects a sidecar binary.
//
// Tauri requires external binaries to be suffixed with the target triple, so the release
// build is copied to src-tauri/binaries/ugly-mcp-<triple>.exe. `npm run tauri:build` runs
// this first; without it the bundle step fails on the missing file.

import { execFileSync } from 'node:child_process';
import { mkdirSync, copyFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = join(dirname(fileURLToPath(import.meta.url)), '..');
const exeSuffix = process.platform === 'win32' ? '.exe' : '';

function run(cmd, args) {
  return execFileSync(cmd, args, { cwd: root, encoding: 'utf8' });
}

const triple = run('rustc', ['-vV'])
  .split('\n')
  .find((line) => line.startsWith('host:'))
  ?.slice('host:'.length)
  .trim();

if (!triple) {
  throw new Error('Could not determine the host target triple from `rustc -vV`.');
}

console.log(`Building ugly-mcp for ${triple}...`);
execFileSync('cargo', ['build', '--release', '-p', 'ugly-mcp'], {
  cwd: root,
  stdio: 'inherit',
});

const from = join(root, 'target', 'release', `ugly-mcp${exeSuffix}`);
const toDir = join(root, 'src-tauri', 'binaries');
const to = join(toDir, `ugly-mcp-${triple}${exeSuffix}`);

mkdirSync(toDir, { recursive: true });
copyFileSync(from, to);
console.log(`Staged ${to}`);
