import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';

const localNoProxy = '127.0.0.1,127.0.0.2,localhost';
const playwrightCli = fileURLToPath(
  new URL('../node_modules/@playwright/test/cli.js', import.meta.url)
);
const args = process.argv.slice(2);
if (args[0] === '--') args.shift();

// Browser engines do not consistently honor wildcard loopback exclusions.
const env = {
  ...process.env,
  NO_PROXY: localNoProxy,
  no_proxy: localNoProxy
};
const result = spawnSync(process.execPath, [playwrightCli, 'test', ...args], {
  env,
  stdio: 'inherit'
});

if (result.error) throw result.error;
process.exit(result.status ?? 1);
