//! Application controller: owns the windows, the tray icon and the keyboard blocker,
//! and keeps them in sync. Runs entirely on the UI thread.

use std::cell::{Cell, RefCell};
use std::path::PathBuf;
use std::rc::Rc;
use std::time::{Duration, Instant};

use keyless_core::{AUTO_RESUME_CHOICES, Appearance, Capture, Edge, Modifiers, Settings, Shortcut};
use keyless_platform::{
    Anchor, Blocker, BlockerEvent, BlockerOptions, Error, PopupKind, autostart, desktop, instance, layout,
};
use slint::winit_030::winit::event::{StartCause, WindowEvent};
use slint::winit_030::winit::event_loop::{ActiveEventLoop, DeviceEvents};
use slint::winit_030::winit::raw_window_handle::HasWindowHandle;
use slint::winit_030::{CustomApplicationHandler, EventResult, WinitWindowAccessor};
use slint::{CloseRequestResponse, ComponentHandle, ModelRc, Timer, TimerMode, VecModel};

use crate::{AppState, Indicator, MainWindow, Panel, Theme, Tray, tray_icons};

const PANEL_WIDTH: f32 = 300.0;
const INDICATOR_HEIGHT: f32 = 44.0;
const RECORDING_TIMEOUT: Duration = Duration::from_secs(30);
/// How often the open main window checks for a keyboard layout change. Windows sends no
/// notice when the layout changes in another app, the taskbar or Win+Space.
const LAYOUT_CHECK: Duration = Duration::from_millis(250);
const REPOSITORY: &str = env!("CARGO_PKG_REPOSITORY");

/// Main window tabs, as numbered in `AppState.page`.
const PAGE_CONTROL: i32 = 0;

/// Bundled languages, in the order of `Choices.languages` in the UI (after "Same as
/// Windows"). Each has a folder in `lang/`, except English, the source language.
const LANGUAGES: [&str; 10] = ["en", "zh", "hi", "es", "ar", "fr", "bn", "pt", "ru", "id"];
/// Written right to left.
const RTL_LANGUAGES: [&str; 1] = ["ar"];
/// Scripts that take extra letter spacing in labels (Latin and Cyrillic).
const TRACKED_LANGUAGES: [&str; 6] = ["en", "es", "fr", "pt", "ru", "id"];

thread_local! {
    static APP: RefCell<Option<Rc<App>>> = const { RefCell::new(None) };
}

/// Runs `f` with the app on the UI thread. Safe to call from any thread.
fn post(f: impl FnOnce(&App) + Send + 'static) {
    let _ = slint::invoke_from_event_loop(move || with_app(f));
}

/// Runs `f` with the app right away. Only for callbacks that already run on the UI thread
/// and must not be delayed, such as dragging the paused tag.
fn with_app(f: impl FnOnce(&App)) {
    if let Some(app) = APP.with(|cell| cell.borrow().clone()) {
        f(&app);
    }
}

pub fn run(background: bool) -> Result<(), String> {
    slint::BackendSelector::new()
        .backend_name("winit".into())
        .renderer_name("software".into())
        .with_winit_custom_application_handler(NoDeviceEvents)
        .select()
        .map_err(|e| format!("Could not start the user interface: {e}"))?;

    let _instance = match instance::acquire(|| post(|app| app.show_main(None))) {
        Ok(Some(guard)) => Some(guard),
        Ok(None) => return Ok(()), // Already running: that copy opens its window.
        Err(_) => None,
    };

    let app = App::new()?;
    APP.with(|cell| *cell.borrow_mut() = Some(app.clone()));
    // Windows only exist once the event loop runs, so finish setup from inside it.
    post(move |app| {
        app.prepare_popups();
        if !background {
            app.show_main(Some(PAGE_CONTROL));
        }
    });
    let result = slint::run_event_loop_until_quit().map_err(|e| e.to_string());
    // Dropping the app removes the keyboard hook and the tray icon.
    APP.with(|cell| cell.borrow_mut().take());
    result
}

/// winit registers for raw keyboard input while one of our windows has focus. Windows then
/// skips this process's own low-level keyboard hook, so keys typed into Keyless itself
/// would not be blocked or recorded. Keyless never uses raw device events, so turn them off.
struct NoDeviceEvents;

impl CustomApplicationHandler for NoDeviceEvents {
    fn new_events(&mut self, event_loop: &ActiveEventLoop, cause: StartCause) -> EventResult {
        if cause == StartCause::Init {
            event_loop.listen_device_events(DeviceEvents::Never);
        }
        EventResult::Propagate
    }
}

struct App {
    main: MainWindow,
    panel: Panel,
    indicator: Indicator,
    tray: Tray,
    blocker: Blocker,
    settings: RefCell<Settings>,
    settings_path: Option<PathBuf>,
    shortcut_available: Cell<bool>,
    recording: Cell<bool>,
    shortcut_error: Cell<ShortcutError>,
    paused_since: Cell<Option<Instant>>,
    resume_timer: Timer,
    countdown_timer: Timer,
    recording_timer: Timer,
    layout_timer: Timer,
    /// The layout the drawn keyboard shows now.
    shown_layout: Cell<Option<usize>>,
    panel_visible: Cell<bool>,
    panel_closed_at: Cell<Option<Instant>>,
}

impl App {
    fn new() -> Result<Rc<Self>, String> {
        let settings_path = directories::ProjectDirs::from("", "", keyless_core::APP_NAME)
            .map(|dirs| dirs.config_dir().join("settings.toml"));
        let settings = settings_path.as_deref().map(Settings::load).unwrap_or_default();

        let options = BlockerOptions { shortcut: None, shortcut_unlocks: settings.shortcut_unlocks };
        let blocker =
            Blocker::start(options, |event| post(move |app| app.on_blocker(event))).map_err(|e| {
                format!("Keyless could not take control of the keyboard ({e}). Your keyboard is unchanged.")
            })?;
        let ui = |e: slint::PlatformError| e.to_string();

        let app = Rc::new(Self {
            main: MainWindow::new().map_err(ui)?,
            panel: Panel::new().map_err(ui)?,
            indicator: Indicator::new().map_err(ui)?,
            tray: Tray::new().map_err(ui)?,
            blocker,
            settings: RefCell::new(settings),
            settings_path,
            shortcut_available: Cell::new(false),
            recording: Cell::new(false),
            shortcut_error: Cell::new(ShortcutError::None),
            paused_since: Cell::new(None),
            resume_timer: Timer::default(),
            countdown_timer: Timer::default(),
            recording_timer: Timer::default(),
            layout_timer: Timer::default(),
            shown_layout: Cell::new(None),
            panel_visible: Cell::new(false),
            panel_closed_at: Cell::new(None),
        });

        let shortcut = app.settings.borrow().shortcut;
        app.reserve_shortcut(shortcut, false);
        app.apply_language();
        app.apply_appearance();
        app.wire();
        app.sync();
        Ok(app)
    }

    // ----- setup -----

    /// Applies the light/dark choice and the system animation setting to the UI, and the
    /// light/dark choice to the native title bars.
    fn apply_appearance(&self) {
        let appearance = self.settings.borrow().appearance;
        let index = Appearance::ALL.iter().position(|&a| a == appearance).unwrap_or(0);
        let animations = desktop::animations_enabled();
        self.for_each_window(|theme, _| {
            theme.set_appearance(index as i32);
            theme.set_animations(animations);
        });
        for window in [self.main.window(), self.panel.window()] {
            apply_native_theme(window, appearance);
        }
    }

    /// Selects the chosen language, or the system's when it is one Keyless has.
    fn apply_language(&self) {
        let chosen = self.settings.borrow().language.clone();
        let base = |tag: &str| tag.split(['-', '_']).next().unwrap_or_default().to_ascii_lowercase();
        let code = chosen
            .as_deref()
            .map(base)
            .or_else(|| {
                sys_locale::get_locales().map(|tag| base(&tag)).find(|c| LANGUAGES.contains(&c.as_str()))
            })
            .filter(|c| LANGUAGES.contains(&c.as_str()))
            .unwrap_or_else(|| "en".into());
        let _ = slint::select_bundled_translation(if code == "en" { "" } else { &code });
        let rtl = RTL_LANGUAGES.contains(&code.as_str());
        let tracking = if TRACKED_LANGUAGES.contains(&code.as_str()) { 1.0 } else { 0.0 };
        let script_font = desktop::font_for_language(&code).unwrap_or_default();
        self.for_each_window(|theme, _| {
            theme.set_rtl(rtl);
            theme.set_tracking(tracking);
            theme.set_script_font(script_font.into());
        });
    }

    fn wire(self: &Rc<Self>) {
        self.for_each_window(|_, state: AppState| {
            state.on_toggle(|| post(|app| app.blocker.set_locked(!app.blocker.is_locked())));
            state.on_resume(|| post(|app| app.blocker.set_locked(false)));
            state.on_open_page(|page| post(move |app| app.show_main(Some(page))));
            state.on_open_source(|| desktop::open_url(REPOSITORY));
            state.on_open_issues(|| desktop::open_url(&format!("{REPOSITORY}/issues")));
            state.on_open_licenses(open_licenses);
            state.on_quit(|| post(App::quit));
            state.on_record_shortcut(|| post(App::start_recording));
            state.on_cancel_recording(|| post(|app| app.stop_recording(ShortcutError::None)));
            state.on_save_shortcut(|text| {
                let text = text.to_string();
                post(move |app| app.save_clicked_shortcut(&text));
            });
            state.on_shortcut_unlocks_changed(|value| post(move |app| app.set_shortcut_unlocks(value)));
            state.on_auto_resume_changed(|index| post(move |app| app.set_auto_resume(index)));
            state.on_show_indicator_changed(|value| post(move |app| app.set_show_indicator(value)));
            state.on_indicator_edge_changed(|index| post(move |app| app.set_indicator_edge(index)));
            state.on_indicator_reset(|| post(App::reset_indicator_spot));
            state.on_drag_indicator(|dx, dy| with_app(|app| app.drag_indicator(dx, dy)));
            state.on_indicator_dropped(|| with_app(App::drop_indicator));
            state.on_start_at_login_changed(|value| post(move |app| app.set_start_at_login(value)));
            state.on_appearance_changed(|index| post(move |app| app.set_appearance(index)));
            state.on_language_changed(|index| post(move |app| app.set_language(index)));
        });

        self.main.window().on_close_requested(|| {
            post(|app| {
                app.main.global::<AppState>().set_window_open(false);
                app.layout_timer.stop();
            });
            CloseRequestResponse::HideWindow
        });

        self.panel.on_dismiss(|| post(App::hide_panel));
        self.panel.window().on_winit_window_event(|_, event| {
            if let WindowEvent::Focused(false) = event {
                post(App::hide_panel);
            }
            EventResult::Propagate
        });

        self.tray.set_dev(keyless_core::DEV_BUILD);
        self.tray.set_icon_on(tray_icons::on());
        self.tray.set_icon_paused(tray_icons::paused());
        self.tray.on_open_panel(|| post(App::toggle_panel));
        self.tray.on_toggle(|| post(|app| app.blocker.set_locked(!app.blocker.is_locked())));
        self.tray.on_open_page(|page| post(move |app| app.show_main(Some(page))));
        self.tray.on_quit(|| post(App::quit));
    }

    /// Applies popup styling before the first show, so popups never flash in the taskbar
    /// and the paused tag never steals focus.
    ///
    /// The tag is shown once off-screen so it is fully set up, then only ever shown and
    /// hidden passively: showing it must never take focus from the app in use.
    fn prepare_popups(&self) {
        if let Some(handle) = raw_handle(self.panel.window()) {
            desktop::style_popup(handle, PopupKind::Panel);
        }
        let indicator = self.indicator.window();
        indicator.set_position(slint::PhysicalPosition::new(-32000, -32000));
        if let Some(handle) = raw_handle(indicator) {
            desktop::style_popup(handle, PopupKind::Overlay);
        }
        let _ = indicator.show();
        if let Some(handle) = raw_handle(indicator) {
            desktop::set_visible_passive(handle, false);
        }
    }

    fn for_each_window(&self, f: impl Fn(Theme<'_>, AppState<'_>)) {
        f(self.main.global(), self.main.global());
        f(self.panel.global(), self.panel.global());
        f(self.indicator.global(), self.indicator.global());
    }

    /// Pushes the current state into every window and the tray icon.
    fn sync(&self) {
        let settings = self.settings.borrow();
        let paused = self.blocker.is_locked();
        let shortcut = settings.shortcut;
        let parts = shortcut.parts(desktop::SUPER_KEY_NAME);
        let has = |m| shortcut.modifiers.contains(m);
        let auto_index =
            AUTO_RESUME_CHOICES.iter().position(|&m| m == settings.auto_resume_minutes).unwrap_or(0);
        let edge_index = Edge::ALL.iter().position(|&e| e == settings.indicator_edge).unwrap_or(0);
        let appearance_index = Appearance::ALL.iter().position(|&a| a == settings.appearance).unwrap_or(0);
        let language_index = settings
            .language
            .as_deref()
            .and_then(|code| LANGUAGES.iter().position(|&l| l == code))
            .map_or(0, |i| i + 1);
        let (error, error_name) = match self.shortcut_error.get() {
            ShortcutError::None => (0, String::new()),
            ShortcutError::Invalid => (1, String::new()),
            ShortcutError::Taken(shortcut) => (2, shortcut.parts(desktop::SUPER_KEY_NAME).join(" + ")),
            ShortcutError::Failed => (3, String::new()),
        };
        let start_at_login = autostart::is_enabled();
        self.for_each_window(|_, state| {
            state.set_paused(paused);
            state.set_shortcut_text(parts.join(" + ").into());
            state.set_sc_ctrl(has(Modifiers::CTRL));
            state.set_sc_alt(has(Modifiers::ALT));
            state.set_sc_shift(has(Modifiers::SHIFT));
            state.set_sc_win(has(Modifiers::SUPER));
            state.set_sc_key(parts.last().map(String::as_str).unwrap_or_default().into());
            state.set_super_name(desktop::SUPER_KEY_NAME.into());
            state.set_shortcut_available(self.shortcut_available.get());
            state.set_recording(self.recording.get());
            state.set_shortcut_error(error);
            state.set_shortcut_error_name(error_name.as_str().into());
            state.set_shortcut_unlocks(settings.shortcut_unlocks);
            state.set_auto_resume_index(auto_index as i32);
            state.set_show_indicator(settings.show_indicator);
            state.set_indicator_edge(edge_index as i32);
            state.set_indicator_custom(settings.indicator_spot.is_some());
            state.set_start_at_login(start_at_login);
            state.set_appearance_index(appearance_index as i32);
            state.set_language_index(language_index as i32);
            state.set_version(env!("CARGO_PKG_VERSION").into());
            state.set_beta(keyless_core::is_prerelease(env!("CARGO_PKG_VERSION")));
            state.set_dev(keyless_core::DEV_BUILD);
        });
        drop(settings);
        self.update_countdown();
        self.tray.set_paused(paused);
    }

    /// Labels the drawn keyboard for the active keyboard layout, if it changed.
    fn refresh_key_caps(&self) {
        let active = layout::active_layout();
        if self.shown_layout.replace(Some(active)) == Some(active) {
            return;
        }
        let model: Vec<crate::KeyCap> = layout::key_caps()
            .iter()
            .map(|cap| crate::KeyCap { label: cap.label.as_str().into(), usable: cap.key.is_some() })
            .collect();
        self.main.global::<AppState>().set_key_caps(ModelRc::new(VecModel::from(model)));
    }

    fn save(&self) {
        if let Some(path) = &self.settings_path {
            let _ = self.settings.borrow().save(path);
        }
    }

    // ----- keyboard state -----

    fn on_blocker(&self, event: BlockerEvent) {
        match event {
            BlockerEvent::LockChanged(locked) => self.on_lock_changed(locked),
            BlockerEvent::Captured(Capture::Shortcut(shortcut)) => {
                self.stop_recording(ShortcutError::None);
                self.reserve_shortcut(shortcut, true);
                self.sync();
            }
            BlockerEvent::Captured(Capture::Invalid) => self.stop_recording(ShortcutError::Invalid),
        }
    }

    fn on_lock_changed(&self, locked: bool) {
        if locked {
            self.paused_since.set(Some(Instant::now()));
            self.schedule_auto_resume();
        } else {
            self.paused_since.set(None);
            self.resume_timer.stop();
            self.countdown_timer.stop();
        }
        self.update_indicator();
        self.sync();
    }

    fn schedule_auto_resume(&self) {
        self.resume_timer.stop();
        self.countdown_timer.stop();
        let minutes = self.settings.borrow().auto_resume_minutes;
        let Some(since) = self.paused_since.get() else { return };
        if minutes == 0 {
            return;
        }
        let remaining = Duration::from_secs(u64::from(minutes) * 60).saturating_sub(since.elapsed());
        self.resume_timer.start(TimerMode::SingleShot, remaining, || {
            post(|app| app.blocker.set_locked(false));
        });
        // Ticks the visible countdown; runs only while paused with auto-resume on.
        self.countdown_timer
            .start(TimerMode::Repeated, Duration::from_secs(1), || post(App::update_countdown));
    }

    /// Updates the "14:32" countdown and the auto-resume progress.
    fn update_countdown(&self) {
        let minutes = self.settings.borrow().auto_resume_minutes;
        let (text, progress) = match self.paused_since.get() {
            Some(since) if minutes > 0 => {
                let total = Duration::from_secs(u64::from(minutes) * 60);
                let left = total.saturating_sub(since.elapsed()).as_secs();
                let progress = (since.elapsed().as_secs_f32() / total.as_secs_f32()).min(1.0);
                (format!("{}:{:02}", left / 60, left % 60), progress)
            }
            _ => (String::new(), 0.0),
        };
        self.for_each_window(|_, state| {
            state.set_countdown(text.as_str().into());
            state.set_progress(progress);
        });
    }

    // ----- paused tag -----

    /// Logical size of the paused tag, which grows with longer translations.
    fn indicator_size(&self) -> (f32, f32) {
        (self.indicator.get_content_width(), INDICATOR_HEIGHT)
    }

    fn update_indicator(&self) {
        let window = self.indicator.window();
        let Some(handle) = raw_handle(window) else { return };
        let visible = self.blocker.is_locked() && self.settings.borrow().show_indicator;
        if visible {
            if let Some((x, y)) = self.indicator_position() {
                window.set_position(slint::PhysicalPosition::new(x, y));
            }
            window.request_redraw();
        }
        desktop::set_visible_passive(handle, visible);
    }

    /// The dragged-to spot while it is still on a screen, otherwise the chosen edge.
    fn indicator_position(&self) -> Option<(i32, i32)> {
        let settings = self.settings.borrow();
        settings
            .indicator_spot
            .and_then(|spot| desktop::fit(spot, self.indicator_size()))
            .or_else(|| desktop::place(Anchor::Edge(settings.indicator_edge), self.indicator_size()))
    }

    /// Moves the tag with the pointer. `dx`, `dy`: logical offset since the press.
    fn drag_indicator(&self, dx: f32, dy: f32) {
        let window = self.indicator.window();
        let scale = window.scale_factor();
        let position = window.position();
        window.set_position(slint::PhysicalPosition::new(
            position.x + (dx * scale).round() as i32,
            position.y + (dy * scale).round() as i32,
        ));
    }

    /// Remembers where the tag was dropped, kept fully on screen.
    fn drop_indicator(&self) {
        let window = self.indicator.window();
        let position = window.position();
        let spot =
            desktop::fit((position.x, position.y), self.indicator_size()).unwrap_or((position.x, position.y));
        if spot != (position.x, position.y) {
            window.set_position(slint::PhysicalPosition::new(spot.0, spot.1));
        }
        self.settings.borrow_mut().indicator_spot = Some(spot);
        self.save();
        self.sync();
    }

    fn set_indicator_edge(&self, index: i32) {
        let Some(&edge) = usize::try_from(index).ok().and_then(|i| Edge::ALL.get(i)) else { return };
        {
            let mut settings = self.settings.borrow_mut();
            settings.indicator_edge = edge;
            settings.indicator_spot = None;
        }
        self.save();
        self.update_indicator();
        self.sync();
    }

    fn reset_indicator_spot(&self) {
        self.settings.borrow_mut().indicator_spot = None;
        self.save();
        self.update_indicator();
        self.sync();
    }

    // ----- shortcut -----

    fn reserve_shortcut(&self, shortcut: Shortcut, save: bool) {
        let unchanged = self.shortcut_available.get() && self.settings.borrow().shortcut == shortcut;
        if unchanged {
            self.shortcut_error.set(ShortcutError::None);
            return;
        }
        match self.blocker.set_shortcut(Some(shortcut)) {
            Ok(()) => {
                self.shortcut_available.set(true);
                self.settings.borrow_mut().shortcut = shortcut;
                self.shortcut_error.set(ShortcutError::None);
                if save {
                    self.save();
                }
            }
            Err(Error::ShortcutTaken) => self.shortcut_error.set(ShortcutError::Taken(shortcut)),
            Err(_) => self.shortcut_error.set(ShortcutError::Failed),
        }
    }

    /// A shortcut set by clicking keys, as "Ctrl+Alt+K".
    fn save_clicked_shortcut(&self, text: &str) {
        match text.parse::<Shortcut>() {
            Ok(shortcut) if shortcut.is_valid() => self.reserve_shortcut(shortcut, true),
            _ => self.shortcut_error.set(ShortcutError::Invalid),
        }
        self.sync();
    }

    fn start_recording(&self) {
        self.recording.set(true);
        self.shortcut_error.set(ShortcutError::None);
        self.blocker.set_recording(true);
        self.recording_timer.start(TimerMode::SingleShot, RECORDING_TIMEOUT, || {
            post(|app| app.stop_recording(ShortcutError::None));
        });
        self.sync();
    }

    fn stop_recording(&self, error: ShortcutError) {
        self.recording_timer.stop();
        self.blocker.set_recording(false);
        self.recording.set(false);
        self.shortcut_error.set(error);
        self.sync();
    }

    // ----- settings -----

    fn set_shortcut_unlocks(&self, value: bool) {
        self.settings.borrow_mut().shortcut_unlocks = value;
        self.blocker.set_shortcut_unlocks(value);
        self.save();
        self.sync();
    }

    fn set_show_indicator(&self, value: bool) {
        self.settings.borrow_mut().show_indicator = value;
        self.save();
        self.update_indicator();
        self.sync();
    }

    fn set_auto_resume(&self, index: i32) {
        let minutes =
            usize::try_from(index).ok().and_then(|i| AUTO_RESUME_CHOICES.get(i)).copied().unwrap_or(0);
        self.settings.borrow_mut().auto_resume_minutes = minutes;
        self.save();
        if self.paused_since.get().is_some() {
            // A new choice counts from now.
            self.paused_since.set(Some(Instant::now()));
            self.schedule_auto_resume();
        }
        self.sync();
    }

    fn set_start_at_login(&self, value: bool) {
        let _ = autostart::set_enabled(value);
        self.sync();
    }

    fn set_appearance(&self, index: i32) {
        let appearance =
            usize::try_from(index).ok().and_then(|i| Appearance::ALL.get(i)).copied().unwrap_or_default();
        self.settings.borrow_mut().appearance = appearance;
        self.save();
        self.apply_appearance();
        self.sync();
    }

    fn set_language(&self, index: i32) {
        let language = usize::try_from(index - 1).ok().and_then(|i| LANGUAGES.get(i)).map(|&l| l.to_owned());
        self.settings.borrow_mut().language = language;
        self.save();
        self.apply_language();
        self.sync();
        // The tag's width depends on the language.
        self.update_indicator();
    }

    // ----- windows -----

    fn toggle_panel(&self) {
        if self.panel_visible.get() {
            self.hide_panel();
            return;
        }
        // A tray click that took focus away from the open panel already closed it.
        if self.panel_closed_at.get().is_some_and(|t| t.elapsed() < Duration::from_millis(400)) {
            return;
        }
        let window = self.panel.window();
        let size = (PANEL_WIDTH, self.panel.get_content_height());
        if let Some((x, y)) = desktop::place(Anchor::Pointer, size) {
            window.set_position(slint::PhysicalPosition::new(x, y));
        }
        let _ = window.show();
        apply_native_theme(window, self.settings.borrow().appearance);
        if let Some(handle) = raw_handle(window) {
            // Idempotent; also covers a native window that only came to exist with this show.
            desktop::style_popup(handle, PopupKind::Panel);
            desktop::bring_to_front(handle);
        }
        self.panel_visible.set(true);
    }

    fn hide_panel(&self) {
        if self.panel_visible.replace(false) {
            self.panel_closed_at.set(Some(Instant::now()));
            let _ = self.panel.hide();
        }
    }

    /// Opens the main window, on `page` if given.
    fn show_main(&self, page: Option<i32>) {
        self.hide_panel();
        // Pick up a changed "Show animations" setting.
        self.apply_appearance();
        let state = self.main.global::<AppState>();
        if let Some(page) = page {
            state.set_page(page);
        }
        state.set_window_open(true);
        self.refresh_key_caps();
        if !self.layout_timer.running() {
            self.layout_timer.start(TimerMode::Repeated, LAYOUT_CHECK, || with_app(App::refresh_key_caps));
        }
        let window = self.main.window();
        let _ = self.main.show();
        apply_native_theme(window, self.settings.borrow().appearance);
        set_app_icon(window);
        if let Some(handle) = raw_handle(window) {
            desktop::bring_to_front(handle);
        }
    }

    fn quit(&self) {
        self.blocker.set_locked(false);
        let _ = self.panel.hide();
        if let Some(handle) = raw_handle(self.indicator.window()) {
            desktop::set_visible_passive(handle, false);
        }
        let _ = self.main.hide();
        let _ = self.tray.hide();
        let _ = slint::quit_event_loop();
    }
}

/// Why the last shortcut change failed. The UI words it in the current language.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ShortcutError {
    None,
    /// Not a usable combination.
    Invalid,
    /// Another app or Windows already uses it.
    Taken(Shortcut),
    Failed,
}

/// Opens the third-party license list shipped next to the executable, or the project
/// page when running a build without it.
fn open_licenses() {
    let local = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|dir| dir.join("THIRD_PARTY_LICENSES.html")))
        .filter(|path| path.exists());
    match local {
        Some(path) => desktop::open_url(&path.to_string_lossy()),
        None => desktop::open_url(REPOSITORY),
    }
}

fn raw_handle(window: &slint::Window) -> Option<slint::winit_030::winit::raw_window_handle::RawWindowHandle> {
    window.with_winit_window(|w| w.window_handle().ok().map(|h| h.as_raw())).flatten()
}

/// Matches the native title bar to the chosen appearance. `System` hands control back to the OS.
fn apply_native_theme(window: &slint::Window, appearance: Appearance) {
    use slint::winit_030::winit::window::Theme as NativeTheme;
    let theme = match appearance {
        Appearance::System => None,
        Appearance::Light => Some(NativeTheme::Light),
        Appearance::Dark => Some(NativeTheme::Dark),
    };
    window.with_winit_window(|w| w.set_theme(theme));
}

/// Uses the icon embedded in the executable for the title bar and taskbar.
/// winit does not pick it up on its own. Dev builds show their tinted tray face instead.
#[cfg(windows)]
fn set_app_icon(window: &slint::Window) {
    use slint::winit_030::winit::dpi::PhysicalSize;
    use slint::winit_030::winit::platform::windows::{IconExtWindows, WindowExtWindows};
    use slint::winit_030::winit::window::Icon;
    const ICON_ID: u16 = 1; // winresource's default application icon id
    if keyless_core::DEV_BUILD {
        let icon = tray_icons::on()
            .to_rgba8()
            .and_then(|face| Icon::from_rgba(face.as_bytes().to_vec(), face.width(), face.height()).ok());
        window.with_winit_window(|w| {
            w.set_window_icon(icon.clone());
            w.set_taskbar_icon(icon.clone());
        });
        return;
    }
    window.with_winit_window(|w| {
        let small = (16.0 * w.scale_factor()).round() as u32;
        w.set_window_icon(Icon::from_resource(ICON_ID, Some(PhysicalSize::new(small, small))).ok());
        w.set_taskbar_icon(Icon::from_resource(ICON_ID, None).ok());
    });
}

#[cfg(not(windows))]
fn set_app_icon(_window: &slint::Window) {}
