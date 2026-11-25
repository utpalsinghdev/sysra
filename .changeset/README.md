# Changesets

For user-facing changes, run `npm ci` and `npm run changeset` with Node.js 22 or later. Select `sysra`, choose a version bump, and describe the change for the changelog. Include the new Markdown file in the same commit as the change.

To prepare a release, run `npm run version:release`. Changesets consumes the pending entries and updates `CHANGELOG.md` and `package.json`. The script synchronizes `Cargo.toml`, `Cargo.lock`, and `package-lock.json`. Review and commit those files together before tagging the release.

The npm package is private and only tracks the release version. Publish the Rust binary through the [tag workflow](../.github/workflows/release.yml), not through npm.
