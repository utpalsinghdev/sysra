# Changesets

For user-facing changes, run `npm ci` and `npm run changeset` with Node.js 22 or later. Select `sysra`, choose a version bump, and describe the change for the changelog. Include the new Markdown file in the same commit as the change.

To prepare a release, run `npm run version:release`. Changesets consumes the pending entries and updates `CHANGELOG.md` and `package.json`. The script synchronizes `Cargo.toml`, `Cargo.lock`, and `package-lock.json`. Review and commit those files together before tagging the release.

## Publish a release

The npm package is private and only tracks the release version. After pushing the version commit to `main`, tag that commit with the version in `Cargo.toml`. For example, if the version is `0.1.1`:

```sh
git tag -a v0.1.1 -m "sysra v0.1.1"
git push origin v0.1.1
```

The [release workflow](../.github/workflows/release.yml) attaches four Rust binary archives and their SHA-256 checksums to a GitHub release. Run the workflow manually on a branch to build artifacts without publishing.
