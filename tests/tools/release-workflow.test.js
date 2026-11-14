const assert = require('node:assert/strict');
const { spawnSync } = require('node:child_process');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const { test } = require('node:test');

const workflow = fs.readFileSync(path.resolve(__dirname, '../../.github/workflows/release.yml'), 'utf8').replace(/\r\n/g, '\n');
function jobBlock(name) {
  const match = workflow.match(new RegExp(`^  ${name}:\\n([\\s\\S]*?)(?=^  [a-z][a-z0-9-]*:|$(?![\\s\\S]))`, 'm'));
  assert.ok(match, `Missing job: ${name}`);
  return match[1];
}

// Read the actual shell run blocks at the workflow's step indentation.
function runBlocks(block) {
  const lines = block.split('\n');
  const commands = [];
  for (let index = 0; index < lines.length; index++) {
    const match = lines[index].match(/^        run: (.*)$/);
    if (!match) continue;
    if (match[1] !== '|') { commands.push(match[1]); continue; }
    const body = [];
    while (++index < lines.length && (lines[index].startsWith('          ') || !lines[index].trim())) {
      body.push(lines[index].slice(10));
    }
    index--;
    commands.push(body.join('\n'));
  }
  assert.ok(commands.length, 'No workflow commands found');
  return commands;
}

const linuxOnly = { skip: process.platform !== 'linux' };
test('release remains a draft until both accepted builds succeed', linuxOnly, (t) => {
  const temporary = fs.mkdtempSync(path.join(os.tmpdir(), 'kb-release-workflow-'));
  t.after(() => fs.rmSync(temporary, { recursive: true, force: true }));
  const state = path.join(temporary, 'state.json');
  const stub = path.join(temporary, 'gh');
  fs.writeFileSync(stub, `#!/usr/bin/env node
const fs = require('node:fs');
const file = process.env.KB_RELEASE_TEST_STATE;
const state = JSON.parse(fs.readFileSync(file, 'utf8'));
const args = process.argv.slice(2);
if (args[1] === 'view') {
  if (!state.exists) process.exit(1);
  console.log(state.draft ? 'true' : 'false');
} else {
  state.mutations.push(args);
  if (args[1] === 'create') { state.exists = true; state.draft = args.includes('--draft'); }
  if (args[1] === 'edit') { state.draft = !args.includes('--draft=false'); }
  fs.writeFileSync(file, JSON.stringify(state));
}
`);
  fs.chmodSync(stub, 0o755);
  fs.writeFileSync(path.join(temporary, 'Cargo.toml'), '[workspace.package]\r\nversion = "9.8.7"\r\n');
  const env = { ...process.env, PATH: `${temporary}:${process.env.PATH}`, RELEASE_TAG: 'v9.8.7',
    GH_REPO: 'local/review', GH_TOKEN: '', KB_RELEASE_TEST_STATE: state };
  const prepare = runBlocks(jobBlock('prepare-release')).join('\n');
  function run(command, tag = 'v9.8.7') {
    return spawnSync('bash', ['-e', '-o', 'pipefail', '-c', command], {
      cwd: temporary, env: { ...env, RELEASE_TAG: tag }, encoding: 'utf8', timeout: 15000,
    });
  }
  function reset(exists, draft) { fs.writeFileSync(state, JSON.stringify({ exists, draft, mutations: [] })); }
  function readState() { return JSON.parse(fs.readFileSync(state, 'utf8')); }

  reset(false, false);
  const created = run(prepare);
  assert.equal(created.status, 0, created.stdout + created.stderr);
  assert.equal(readState().draft, true, 'prepare must not expose an untested release');

  const publish = jobBlock('publish-release');
  assert.match(publish, /needs:\s*\[build-linux, build-windows\]/);
  assert.doesNotMatch(publish, /always\(\)/);
  for (const name of ['build-linux', 'build-windows']) {
    const block = jobBlock(name);
    assert.match(block, /needs:\s*- prepare-release/);
    const acceptance = block.indexOf('test-package-environment.js');
    assert.ok(acceptance >= 0, `${name} must run package acceptance`);
    assert.ok(acceptance < block.indexOf('gh release upload'));
  }
  // A failed build cannot reach the final job under the asserted success dependencies.
  assert.equal(readState().draft, true);
  const published = run(runBlocks(publish).join('\n'));
  assert.equal(published.status, 0, published.stdout + published.stderr);
  assert.equal(readState().draft, false);

  reset(false, false);
  assert.notEqual(run(prepare, 'v9.8.8').status, 0);
  assert.deepEqual(readState().mutations, [], 'version mismatch must precede every release mutation');
  assert.notEqual(run(prepare, 'invalid-tag').status, 0);
  assert.deepEqual(readState().mutations, []);
  reset(true, false);
  assert.notEqual(run(prepare).status, 0, 'published releases must not be overwritten');
  assert.deepEqual(readState().mutations, []);
  reset(true, true);
  assert.equal(run(prepare).status, 0, 'existing drafts may be retried');
  assert.deepEqual(readState().mutations, []);
});
