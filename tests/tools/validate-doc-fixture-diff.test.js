const assert = require('node:assert/strict');
const { spawnSync } = require('node:child_process');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const { test } = require('node:test');

const repoRoot = path.resolve(__dirname, '..', '..');

test('missing command JSON example fails fixture validation', (t) => {
  const workspace = fs.mkdtempSync(path.join(os.tmpdir(), 'kb-doc-fixture-'));
  t.after(() => fs.rmSync(workspace, { recursive: true, force: true }));

  const docsDir = path.join(workspace, 'docs', 'cli');
  const fixtureDir = path.join(workspace, 'tests', 'fixtures', 'cli');
  const toolDir = path.join(workspace, 'tests', 'tools');
  fs.mkdirSync(docsDir, { recursive: true });
  fs.mkdirSync(fixtureDir, { recursive: true });
  fs.mkdirSync(toolDir, { recursive: true });
  fs.copyFileSync(
    path.join(repoRoot, 'docs', 'cli', 'command-examples.md'),
    path.join(docsDir, 'command-examples.md'),
  );
  fs.cpSync(path.join(repoRoot, 'tests', 'fixtures', 'cli'), fixtureDir, { recursive: true });
  fs.copyFileSync(
    path.join(repoRoot, 'tests', 'tools', 'validate-doc-fixture-diff.js'),
    path.join(toolDir, 'validate-doc-fixture-diff.js'),
  );

  const docsPath = path.join(docsDir, 'command-examples.md');
  const docs = fs.readFileSync(docsPath, 'utf8');
  const initStart = docs.indexOf('## 1. `kb init');
  const addStart = docs.indexOf('## 2. `kb add', initStart);
  assert.notEqual(initStart, -1);
  assert.notEqual(addStart, -1);

  const initSection = docs.slice(initStart, addStart);
  const jsonBlock = /```json\r?\n[\s\S]*?\r?\n```/;
  assert.match(initSection, jsonBlock);
  const brokenSection = initSection.replace(jsonBlock, '```text\r\nExample omitted\r\n```');
  fs.writeFileSync(
    docsPath,
    docs.slice(0, initStart) + brokenSection + docs.slice(addStart),
    'utf8',
  );

  const result = spawnSync(
    process.execPath,
    [path.join(toolDir, 'validate-doc-fixture-diff.js')],
    { cwd: workspace, encoding: 'utf8' },
  );
  assert.equal(result.status, 1, result.stdout + result.stderr);
  assert.match(result.stderr, /init[\s\S]*success[\s\S]*JSON example not found/);
});
