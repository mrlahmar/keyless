---
name: release
description: Prepare a new Keyless version (version bump, changelog, local package check). Use when the maintainer asks to prepare or cut a release.
---

# Prepare a release

Versions follow SemVer. Pushing a tag `vX.Y.Z` runs `.github/workflows/release.yml`, which
builds, packages and publishes the GitHub release. Tags starting with `v0.` or containing
`-` are published as pre-releases.

1. Pick the version with the maintainer: fixes are patch, new options are minor.
2. Set `version` under `[workspace.package]` in the root `Cargo.toml`. That is the only
   place; the crates, the exe, the installer and the settings window footer all read it. Run
   `cargo check` so `Cargo.lock` updates.
3. In `CHANGELOG.md`, rename `## [Unreleased]` to `## [X.Y.Z] - YYYY-MM-DD`, add a new
   empty `## [Unreleased]` above it, and update the compare links at the bottom. The
   release notes are taken from this section; the workflow fails if it is missing.
4. Run the checks in `AGENTS.md`, then `scripts\package.ps1` to confirm the installer and
   zip build (needs Inno Setup 6, or pass `-SkipInstaller`). Delete `dist/` afterwards.
5. Stop there. Committing, tagging (`git tag vX.Y.Z`) and pushing are the maintainer's
   steps unless they ask you to do them. If asked, the commit message is
   `chore(release): vX.Y.Z`.

The workflow refuses a tag that doesn't match the `Cargo.toml` version.
