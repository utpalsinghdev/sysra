import { execFileSync } from 'node:child_process';
import { readFileSync, writeFileSync } from 'node:fs';
import metadata from '../package.json' with { type: 'json' };

const manifest = 'Cargo.toml';
const cargo = readFileSync(manifest, 'utf8');
const { version } = metadata;
const packageVersion = /^(\[package\]\r?\nname = "sysra"\r?\nversion = ")[^"]+(")/;

if (!/^\d+\.\d+\.\d+$/.test(version) || !packageVersion.test(cargo)) {
  throw new Error('Expected a stable sysra version in package.json and Cargo.toml');
}

const updated = cargo.replace(packageVersion, (_, start, end) => start + version + end);
if (updated !== cargo) writeFileSync(manifest, updated);
execFileSync('cargo', ['update', '--workspace', '--offline'], { stdio: 'inherit' });
