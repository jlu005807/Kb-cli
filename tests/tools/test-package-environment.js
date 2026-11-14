const assert = require('node:assert/strict');
const { spawn } = require('node:child_process');
const fs = require('node:fs');
const http = require('node:http');
const path = require('node:path');

async function main() {
  const archive = path.resolve(process.argv[2]);
  assert.ok(fs.existsSync(archive), `Package does not exist: ${archive}`);
  const repoRoot = path.resolve(__dirname, '..', '..');
  let requests = 0;
  const server = http.createServer((request, response) => {
    requests++;
    request.resume();
    response.writeHead(500, { 'Content-Type': 'application/json' });
    response.end('{"error":{"message":"unexpected model request during package acceptance"}}');
  });
  await new Promise((resolve) => server.listen(0, '127.0.0.1', resolve));
  try {
    const env = Object.fromEntries(Object.entries(process.env).filter(([key]) => !key.startsWith('KB_LLM_')));
    Object.assign(env, {
      KB_LLM_KEY: 'local-test-only', KB_LLM_ENABLE_SEMANTIC: '1',
      KB_LLM_BASE_URL: `http://127.0.0.1:${server.address().port}/v1`,
      KB_LLM_TIMEOUT_SECS: '2', KB_LLM_RETRY_MAX_ATTEMPTS: '1',
      KB_QUIET: '1', KB_VERBOSE: '3', KB_NO_COLOR: '1',
      KB_UPDATE_SIMULATE_LAYER_FAILURES: 'semantic', KB_ASK_OFFLINE_STUB: '1',
    });
    const windows = process.platform === 'win32';
    const args = windows
      ? ['-NoProfile', '-ExecutionPolicy', 'Bypass', '-File', 'tests/scripts/test-windows-installer.ps1', '-PackageDir', archive, '-RunCliSmoke']
      : ['tests/scripts/test-linux-installer.sh', archive];
    const status = await new Promise((resolve, reject) => {
      const child = spawn(windows ? 'powershell.exe' : 'bash', args, { cwd: repoRoot, env, stdio: 'inherit' });
      const watchdog = setTimeout(() => { child.kill(); reject(new Error('Package acceptance exceeded 120 seconds')); }, 120000);
      child.on('error', (error) => { clearTimeout(watchdog); reject(error); });
      child.on('exit', (code) => { clearTimeout(watchdog); resolve(code); });
    });
    assert.equal(requests, 0, 'Package acceptance called a model using inherited configuration');
    assert.equal(status, 0, 'Package acceptance failed in a polluted environment');
    console.log('Polluted-environment package acceptance passed; no model requests.');
  } finally {
    await new Promise((resolve) => server.close(resolve));
  }
}

main().catch((error) => { console.error(error.message); process.exitCode = 1; });
