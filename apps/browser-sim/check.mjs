import { readdirSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { spawnSync } from 'node:child_process';
const root = fileURLToPath(new URL('./', import.meta.url));
for (const name of readdirSync(root).filter(name => /^test-.*\.cjs$/.test(name)).sort()) {
  const result = spawnSync(process.execPath, [name], {cwd: root, stdio: 'inherit'});
  if (result.error) throw result.error;
  if (result.status !== 0) process.exit(result.status ?? 1);
}
