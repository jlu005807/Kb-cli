const assert = require('node:assert/strict');
const { spawnSync } = require('node:child_process');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const { test } = require('node:test');

const repoRoot = path.resolve(__dirname, '..', '..');
const windows = process.platform === 'win32';
const platform = windows ? 'windows' : 'linux';

for (const relative of [false, true]) {
  test(`packaging reads binary from ${relative ? 'relative' : 'absolute'} CARGO_TARGET_DIR`, (t) => {
    const workspace = fs.mkdtempSync(path.join(os.tmpdir(), 'kb-package-'));
    t.after(() => fs.rmSync(workspace, { recursive: true, force: true }));
    fs.mkdirSync(path.join(workspace, 'scripts'));
    fs.mkdirSync(path.join(workspace, 'packaging'));
    fs.cpSync(path.join(repoRoot, 'packaging', platform), path.join(workspace, 'packaging', platform), { recursive: true });
    const script = windows ? 'package.ps1' : 'package.sh';
    fs.copyFileSync(path.join(repoRoot, 'scripts', script), path.join(workspace, 'scripts', script));
    fs.writeFileSync(path.join(workspace, 'Cargo.toml'), '[workspace.package]\r\nversion = "9.8.7"\r\n');
    const targetDir = path.join(workspace, 'build output');
    const releaseDir = path.join(targetDir, 'release');
    fs.mkdirSync(releaseDir, { recursive: true });
    const binary = 'binary from custom target directory';
    fs.writeFileSync(path.join(releaseDir, windows ? 'kb-cli.exe' : 'kb-cli'), binary);
    const args = windows
      ? ['-NoProfile', '-ExecutionPolicy', 'Bypass', '-File', path.join('scripts', script), '-SkipBuild']
      : [path.join('scripts', script), '--skip-build'];
    const result = spawnSync(windows ? 'powershell.exe' : 'bash', args, {
      cwd: workspace,
      env: { ...process.env, CARGO_TARGET_DIR: relative ? 'build output' : targetDir },
      encoding: 'utf8',
      timeout: 60000,
    });
    assert.equal(result.status, 0, result.stdout + result.stderr);
    const stage = fs.readdirSync(path.join(workspace, 'dist')).find((name) => fs.statSync(path.join(workspace, 'dist', name)).isDirectory());
    assert.match(stage, new RegExp(`^kb-cli-9\\.8\\.7-${platform}-`));
    const stageDir = path.join(workspace, 'dist', stage);
    assert.equal(fs.readFileSync(path.join(stageDir, windows ? 'kb.exe' : 'kb'), 'utf8'), binary);
    assert.ok(fs.existsSync(path.join(stageDir, 'INSTALL.md')));
    assert.ok(fs.existsSync(path.join(stageDir, windows ? 'install.ps1' : 'install.sh')));
    assert.ok(fs.existsSync(path.join(stageDir, windows ? 'uninstall.ps1' : 'uninstall.sh')));
    if (windows) {
      assert.ok(fs.existsSync(path.join(stageDir, 'install.cmd')));
      assert.ok(fs.existsSync(path.join(stageDir, 'uninstall.cmd')));
    } else {
      assert.notEqual(fs.statSync(path.join(stageDir, 'kb')).mode & 0o111, 0);
      assert.notEqual(fs.statSync(path.join(stageDir, 'install.sh')).mode & 0o111, 0);
      assert.notEqual(fs.statSync(path.join(stageDir, 'uninstall.sh')).mode & 0o111, 0);
    }
    assert.ok(fs.existsSync(path.join(workspace, 'dist', stage + (windows ? '.zip' : '.tar.gz'))));
  });
}
