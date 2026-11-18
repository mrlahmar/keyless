# Security

## What Keyless does with your keystrokes

To pause the keyboard, Keyless installs a low-level keyboard hook. Every key event passes
through it, and for each one Keyless decides in memory whether to let it through. It
keeps only which keys are currently held down, so that every key press gets its
matching release.

Keyless never records, stores or sends keystrokes. It makes no network connections and
has no telemetry. The only file it writes is its settings file
(`%APPDATA%\Keyless\config\settings.toml` on Windows). The code that handles key events
is in `crates/keyless-platform/src/windows/blocker.rs` and `crates/keyless-core/src/keyboard.rs`.

Release binaries are built by GitHub Actions from the tagged source and published with
build provenance and SHA-256 checksums.

## Reporting a vulnerability

Please report security issues privately through
[GitHub security advisories](https://github.com/mrlahmar/keyless/security/advisories/new),
not in public issues. You should get a reply within a week. Supported versions: the
latest release.
