# Agent guide

Keyless pauses the keyboard while the mouse keeps working. It does only that: don't add
features beyond it without the maintainer asking. Windows and Linux are supported today;
macOS comes later, so keep OS code out of the core and UI.

## Layout

- `crates/keyless-core`: key state machine, shortcut model, settings (TOML). Pure Rust, no
  OS calls, fully unit tested. Put any logic that doesn't need the OS here.
- `crates/keyless-platform`: OS code behind one API. `src/windows/` and `src/linux/` are
  the real implementations; `src/unsupported.rs` is the stub every other OS compiles
  against. When you add an item to one, add it to all three.
- `crates/keyless-app`: Slint UI (`ui/*.slint`) and `src/app.rs`, the controller that owns
  windows, tray and blocker. Runs on the UI thread only.
- `packaging/windows/keyless.iss`: Inno Setup installer. `scripts/package.ps1` builds
  everything that ships into `dist/`. On Linux, `scripts/package-linux.sh` builds the
  `.deb` and AppImage from `packaging/linux/`. See `docs/LINUX.md`.
- `site/index.html`: the project website, one page, no framework. `.github/workflows/pages.yml`
  publishes it to GitHub Pages with the fonts, icon and demo GIFs copied in from the repo.

See `docs/ARCHITECTURE.md` for how blocking works.

## Commands

```sh
cargo fmt --all
cargo clippy --workspace --all-targets
cargo test --workspace
cargo run                      # settings window + tray
cargo run -- --background      # tray only, as at sign-in
```

All three must pass. CI fails only on errors, but keep the code at zero warnings.

## Rules

- Never commit, tag, push or change git remotes unless the maintainer asks.
- The version lives only in the root `Cargo.toml`. User-visible changes get a line under
  `[Unreleased]` in `CHANGELOG.md`.
- Keep idle cost near zero: no polling. Timers may run only while paused with auto-resume
  on, while recording a shortcut, or while the main window is open (`window-open`).
- Colors, sizes and fonts come only from `ui/theme.slint`. Don't invent palettes.
- Every control must work with Tab, Space/Enter and a screen reader (set
  `accessible-role` and `accessible-label`). Use the controls in `ui/controls.slint`.
- UI text and docs: short and plain. No marketing tone.
- All UI text goes through `@tr()` and needs a translation in every `.po` file under
  `crates/keyless-app/lang/` (see CONTRIBUTING.md, Translations). Never build sentences by
  joining strings; use `@tr("... {} ...", value)`.
- Arabic is right to left. Use `Row` (from `controls.slint`) instead of `HorizontalLayout`
  where order matters, and `horizontal-alignment: Theme.start-align` on text. A row that
  contains wrapping text must be a `GridLayout` with `row: 0; col: Theme.rtl ? ... : ...;`
  instead, because a flexbox row with wrapped text loops. Check changes in Arabic.
- Letter spacing is `... * Theme.tracking` so it switches off for scripts it breaks.
- Don't create or change icons unless asked. The app icon's source is `assets/keyless.svg`;
  `assets/keyless.ico` is rendered from it (16 to 256 px, thicker lines at 32 px and below).
- The hook lets injected (software-sent) keys through on purpose. Synthetic input therefore
  can't test blocking: say so and ask the maintainer to test with a real keyboard.
- Keyless always starts with the keyboard on. Never save the paused state.
- Keep winit device events off (`NoDeviceEvents` in `src/app.rs`). When they are on, Windows
  stops sending keys typed into Keyless's own windows to the hook.

## Commits

Only when the maintainer asks. Use [Conventional Commits](https://www.conventionalcommits.org/):

```text
<type>(<scope>): <summary in imperative mood, lowercase, no period>
```

- Types: `feat`, `fix`, `perf`, `refactor`, `test`, `docs`, `build`, `ci`, `chore`.
- Scopes: `core`, `platform`, `ui`, `app`, `installer`, `release`. Leave it out when the
  change spans several.
- Breaking changes: add `!` after the type or scope and a `BREAKING CHANGE:` footer.
- One logical change per commit. Example: `fix(platform): record shortcuts while Keyless has focus`.

## Skills

`.agents/skills/` (mirrored in `.claude/skills/`): `verify-ui` to run the app and capture
its windows, `release` to prepare a version.
