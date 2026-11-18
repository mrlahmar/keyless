---
name: verify-ui
description: Run Keyless on Windows and screenshot its settings window, tray panel or paused indicator to check a UI change. Use after editing ui/*.slint or window code in src/app.rs.
---

# Verify a UI change

1. Stop any running copy, since a second launch only reopens the first one:
   `taskkill /F /IM keyless.exe`
2. Build and start: `cargo build --release`, then start `target\release\keyless.exe`
   without waiting for it. The settings window opens.
3. Capture a window with the script next to this file:

   ```powershell
   .\capture.ps1 -List                          # windows of the running keyless.exe
   .\capture.ps1 -Handle <hwnd> -Out shot.png   # one window, DPI-correct
   .\capture.ps1 -Out screen.png                # whole primary screen
   ```

   Save screenshots in a temporary folder, not in the repo. Only `docs/*.png` belong in the
   repo, and only when the README screenshots should change.
4. Look at the image. Check both appearances: set Appearance to Light and Dark in
   Settings › Display (or edit `appearance` in `%APPDATA%\Keyless\config\settings.toml` and restart).
5. Windows that need a trigger:
   - Tray panel: click the Keyless tray icon. It may be in the overflow area (the `^`
     button). It closes when it loses focus, so capture right after the click.
   - Paused tag: flip the switch to PAUSE on the Control page. It shows at the edge chosen
     in Settings (top by default) or where it was last dragged. Click RESUME to resume.
   - Pages: Control, Shortcut, Settings and About are tabs in the main window's sidebar.
6. Stop the app when done: `taskkill /F /IM keyless.exe`.

Key blocking can't be checked this way, because injected input is never blocked. Ask the
maintainer to test with a real keyboard.
