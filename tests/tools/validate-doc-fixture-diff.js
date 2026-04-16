const fs = require('fs');
const path = require('path');

function readFile(p) {
  return fs.readFileSync(p, 'utf8').replace(/\r\n/g, '\n');
}

const DOC = 'docs/cli/command-examples.md';
if (!fs.existsSync(DOC)) {
  console.error('docs not found:', DOC);
  process.exit(2);
}

const text = readFile(DOC);
const cmds = ['init', 'add', 'mv', 'rm', 'path', 'chmod', 'check', 'update', 'list', 'ask'];
const results = [];

function getJsonBlock(t) {
    if (!t) return null;
    const match = t.match(/```json\n([\s\S]*?)\n```/);
    if (!match) return null;
    return match[1] + '\n';
}

for (const cmd of cmds) {
    const rx = new RegExp('## \\d+\\. `kb ' + cmd + '[^`]*`([\\s\\S]*?)(?=\\n## \\d+\\. `kb|$)', '');
    const m = text.match(rx);
    if (!m) {
        results.push({cmd, state: null, error: 'section not found'});
        continue;
    }
    const section = m[0];
    const parts = section.split(/### (?:Failure Example|失败示例)/);
    const succPart = parts[0];
    const failPart = parts[1] || '';

    const succJson = getJsonBlock(succPart);
    const failJson = getJsonBlock(failPart);

    const checks = [
        {state: 'success', jstr: succJson},
        {state: 'failure', jstr: failJson}
    ];

    for (const c of checks) {
        if (!c.jstr) continue;
        const fixPath = path.join('tests','fixtures','cli',`${cmd}_${c.state}.json`);
        if (!fs.existsSync(fixPath)) {
            results.push({cmd, state: c.state, error: `Missing fixture ${fixPath}`});
            continue;
        }
        const fixContentRaw = readFile(fixPath);
        const fixContent = fixContentRaw.replace(/\r\n/g, '\n');
        const cjstr = c.jstr.replace(/\r\n/g, '\n');

        if (cjstr !== fixContent) {
            results.push({cmd, state: c.state, error: 'Mismatch', expected: fixContent, actual: cjstr});
        } else {
            console.log(`Matched ${cmd} ${c.state}`);
            results.push({cmd, state: c.state, error: null, message: 'Matched perfectly'});
        }
    }
}

const mismatches = results.filter(r => r.error);
if (mismatches.length) {
    console.error(JSON.stringify(mismatches, null, 2));
    process.exit(1);
}

console.log('All fixtures match docs.');
process.exit(0);
