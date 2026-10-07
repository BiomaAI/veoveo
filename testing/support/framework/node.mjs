// The maintained node:test event stream owns selection and execution outcomes.
import {run} from 'node:test';
import {spec} from 'node:test/reporters';
import {writeFile} from 'node:fs/promises';
import {resolve} from 'node:path';
const [file, expected, destination] = process.argv.slice(2);
if (!file || !expected || !destination) throw new Error('Node framework delivery requires file, exact case and private report');
const escaped = expected.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
const stream = run({files: [resolve(file)], testNamePatterns: [`^${escaped}$`]});
let matched = 0, passed = 0, skipped = 0, failed = 0;
stream.on('test:pass', data => {
  if (data.name === expected) {matched++; if (data.skip || data.todo) skipped++; else passed++;}
});
stream.on('test:fail', data => {failed++; if (data.name === expected) matched++;});
stream.compose(spec()).pipe(process.stdout);
await new Promise((resolve, reject) => {stream.on('end', resolve); stream.on('error', reject);});
const outcome = {format: 'veoveo.ai/framework-outcome/v1', framework: 'node', case: expected, matched, passed, skipped, failed};
await writeFile(destination, `${JSON.stringify(outcome)}\n`, {flag: 'wx', mode: 0o600});
if (matched !== 1 || passed !== 1 || skipped !== 0 || failed !== 0) process.exitCode = 1;
