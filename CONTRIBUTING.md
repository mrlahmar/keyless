# Contributing

Thanks for helping. Keyless does one thing: pause the keyboard while the mouse keeps
working. Changes that serve that are welcome; features beyond it usually are not, so
please open an issue before starting on something new.

## Setup

1. Install Rust stable from <https://rustup.rs>.
2. Windows: the default MSVC toolchain needs the "Desktop development with C++" workload
   from Visual Studio Build Tools.
3. `cargo run` starts a debug build with a console for logs. `cargo run -- --background`
   starts in the tray only.

## Before opening a pull request

```sh
cargo fmt --all
cargo clippy --workspace --all-targets
cargo test --workspace
```

- Aim for zero warnings. CI shows warnings but fails only on errors.
- Keep pull requests focused. Describe what you tested by hand, especially keyboard
  behavior, since key blocking can't be tested with synthetic input.
- Add an entry under `[Unreleased]` in `CHANGELOG.md` for user-visible changes.
- Logic that does not need the OS belongs in `keyless-core`, with tests.
- UI text: short, plain, no jargon. Every control must work with Tab, Space/Enter and a
  screen reader.

## Commit messages

Use [Conventional Commits](https://www.conventionalcommits.org/): `type(scope): summary`,
for example `fix(platform): record shortcuts while Keyless has focus`.

- Types: `feat`, `fix`, `perf`, `refactor`, `test`, `docs`, `build`, `ci`, `chore`.
- Scopes (optional): `core`, `platform`, `ui`, `app`, `installer`, `release`.
- Mark breaking changes with `!` (`feat(core)!: ...`).

Pull request titles follow the same format.

## Translations

Translations are gettext files in `crates/keyless-app/lang/<language>/LC_MESSAGES/keyless.po`,
built into the executable. To improve one, edit its `msgstr` lines and keep every `{}`.
Key names (Ctrl, Alt, Shift, Win) and "Keyless" stay as they are.

New UI text goes through `@tr("...")` in the `.slint` files. Then update the template and
add the new strings to every `.po` file:

```sh
cargo install slint-tr-extractor
slint-tr-extractor --no-default-translation-context -o crates/keyless-app/lang/keyless.pot crates/keyless-app/ui/*.slint
```

A new language needs a folder in `lang/` and an entry in `LANGUAGES` in
`crates/keyless-app/src/app.rs` and `Choices.languages` in `ui/app.slint`.

## Porting to macOS or Linux

Read [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md). A port adds one module to
`keyless-platform` that provides the same items as `src/windows/`; the core and UI stay
unchanged. Please open an issue first to agree on the approach for blocking input.

## Releases (maintainers)

1. Update the version in the root `Cargo.toml` and move the `[Unreleased]` notes in
   `CHANGELOG.md` under the new version.
2. Commit (`chore(release): v1.2.3`), then tag: `git tag v1.2.3 && git push origin v1.2.3`.
3. The release workflow builds, packages and publishes the GitHub release. Versions below
   1.0 and versions with a suffix (`v1.3.0-beta.1`) are published as pre-releases.
