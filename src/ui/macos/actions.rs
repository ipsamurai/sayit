//! `Actions` receives every click from the menus and the Settings window.
//! AppKit calls back by selector, so this is an Objective-C class defined in
//! Rust; each method hands off to plain Rust.

use std::cell::{Cell, OnceCell, RefCell};
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::Duration;

use dispatch2::DispatchQueue;
use objc2::rc::Retained;
use objc2::runtime::{AnyObject, NSObject, ProtocolObject};
use objc2::{DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send};
use objc2_app_kit::{
    NSAlert, NSAlertFirstButtonReturn, NSApplication, NSApplicationActivationPolicy,
    NSApplicationDelegate, NSButton, NSColor, NSControlStateValueOn, NSMenu, NSMenuDelegate,
    NSMenuItem, NSPasteboard, NSPasteboardTypeString, NSPopUpButton, NSSegmentedControl, NSSwitch,
    NSWindow, NSWindowDelegate,
};
use objc2_foundation::{NSNotification, NSObjectProtocol, NSString};

use super::settings::{self, ModelRow, ModelsState, SettingsTab, SettingsWindow, Toggle};
use super::setup::{self, HOTKEYS, Setup, refresh_permissions};
use super::widgets::check_state;
use super::{
    DEFAULT_MIC_TAG, MODEL_MENU, RECENT_MENU, fill_mic_menu, fill_model_menu, fill_recent_menu,
    save_config, with_actions, with_ui,
};
use super::{login, permissions};
use crate::audio;
use crate::config::{Config, Mode, OnClose};
use crate::daemon::Controls;
use crate::models::{self, Download, Finished};
use crate::paths;
use crate::stt::ModelId;

pub struct Ivars {
    controls: Arc<Controls>,
    settings: OnceCell<SettingsWindow>,
    /// Whether the permission watcher thread is running.
    watching: Cell<bool>,
    /// A microphone test is recording; further clicks are ignored.
    testing_mic: Cell<bool>,
    setup: OnceCell<Setup>,
    /// Model cards in Settings and in the setup assistant, kept in sync.
    model_rows: RefCell<Vec<ModelRow>>,
    /// At most one model downloads at a time.
    download: RefCell<Option<(ModelId, Download)>>,
    progress: Cell<f64>,
    last_error: RefCell<Option<(ModelId, String)>>,
    on_close: Cell<OnClose>,
    /// The dictations listed in the Recent Dictations menu when it opened.
    recent_shown: RefCell<Vec<String>>,
}

define_class!(
    // SAFETY: NSObject has no subclassing requirements; Actions adds no Drop
    // impl and, being MainThreadOnly, its methods run on the main thread.
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "SayitActions"]
    #[ivars = Ivars]
    pub struct Actions;

    impl Actions {
        #[unsafe(method(togglePause:))]
        fn toggle_pause(&self, item: &NSMenuItem) {
            let paused = !self.controls().paused.load(Ordering::Relaxed);
            self.controls().paused.store(paused, Ordering::Relaxed);
            item.setState(check_state(paused));
            with_ui(|ui| ui.paused.set(paused));
        }

        #[unsafe(method(selectMic:))]
        fn select_mic(&self, item: &NSMenuItem) {
            let choice = (item.tag() != DEFAULT_MIC_TAG).then(|| item.title().to_string());
            *self.controls().input_device.lock().unwrap_or_else(|e| e.into_inner()) =
                choice.clone();
            save_config(|cfg| cfg.input_device = choice);
        }

        #[unsafe(method(selectModel:))]
        fn select_model(&self, item: &NSMenuItem) {
            if let Some(&model) = ModelId::ALL.get(item.tag() as usize) {
                self.use_model(model);
            }
        }

        #[unsafe(method(useModel:))]
        fn use_model_clicked(&self, button: &NSButton) {
            if let Some(&model) = ModelId::ALL.get(button.tag() as usize) {
                self.use_model(model);
            }
        }

        /// Download, or Cancel while this model is downloading.
        #[unsafe(method(downloadModel:))]
        fn download_clicked(&self, button: &NSButton) {
            let Some(&model) = ModelId::ALL.get(button.tag() as usize) else {
                return;
            };
            if let Some((active, download)) = &*self.ivars().download.borrow() {
                if *active == model {
                    download.cancel();
                }
                return;
            }
            let started = models::start(
                model,
                |p| DispatchQueue::main().exec_async(move || with_actions(|a| a.download_progress(p))),
                move |f| {
                    DispatchQueue::main().exec_async(move || with_actions(|a| a.download_finished(model, f)))
                },
            );
            match started {
                Ok(download) => {
                    *self.ivars().last_error.borrow_mut() = None;
                    self.ivars().progress.set(0.0);
                    *self.ivars().download.borrow_mut() = Some((model, download));
                }
                Err(e) => *self.ivars().last_error.borrow_mut() = Some((model, format!("{e:#}"))),
            }
            self.refresh_models();
        }

        #[unsafe(method(deleteModel:))]
        fn delete_clicked(&self, button: &NSButton) {
            let Some(&model) = ModelId::ALL.get(button.tag() as usize) else {
                return;
            };
            if model == self.controls().model() {
                return; // the button is disabled for the model in use
            }
            let alert = NSAlert::new(self.mtm());
            alert.setMessageText(&NSString::from_str(&format!("Delete {}?", model.label())));
            alert.setInformativeText(&NSString::from_str(&format!(
                "This frees {} MB. You can download it again later.",
                model.download_bytes() / 1_000_000
            )));
            alert.addButtonWithTitle(&NSString::from_str("Delete"));
            alert.addButtonWithTitle(&NSString::from_str("Cancel"));
            if alert.runModal() == NSAlertFirstButtonReturn {
                if let Err(e) = models::delete(model) {
                    *self.ivars().last_error.borrow_mut() = Some((model, format!("{e:#}")));
                }
                self.refresh_models();
            }
        }

        #[unsafe(method(toggleLogin:))]
        fn toggle_login(&self, switch: &NSSwitch) {
            if let Err(e) = login::set(switch.state() == NSControlStateValueOn) {
                let alert = NSAlert::new(self.mtm());
                alert.setMessageText(&NSString::from_str("Couldn't change Start at login"));
                alert.setInformativeText(&NSString::from_str(&e));
                alert.runModal();
            }
            self.refresh_login();
        }

        #[unsafe(method(chooseOnClose:))]
        fn choose_on_close(&self, menu: &NSPopUpButton) {
            let Some(&(on_close, _)) = settings::ON_CLOSE.get(menu.indexOfSelectedItem() as usize)
            else {
                return;
            };
            self.apply_on_close(on_close);
            save_config(|cfg| cfg.on_close = on_close);
        }

        #[unsafe(method(chooseHistorySize:))]
        fn choose_history_size(&self, menu: &NSPopUpButton) {
            if let Some(&size) = settings::HISTORY_SIZES.get(menu.indexOfSelectedItem() as usize) {
                self.controls().set_history_size(size);
                save_config(|cfg| cfg.history_size = size);
            }
        }

        /// Copies a recent dictation, as if the user had selected it and
        /// pressed ⌘C.
        #[unsafe(method(copyRecent:))]
        fn copy_recent(&self, item: &NSMenuItem) {
            let shown = self.ivars().recent_shown.borrow();
            if let Some(text) = shown.get(item.tag() as usize) {
                let pasteboard = NSPasteboard::generalPasteboard();
                pasteboard.clearContents();
                // SAFETY: NSPasteboardTypeString is an immutable framework constant.
                let string_type = unsafe { NSPasteboardTypeString };
                pasteboard.setString_forType(&NSString::from_str(text), string_type);
            }
        }

        #[unsafe(method(clearRecent:))]
        fn clear_recent(&self, _sender: Option<&AnyObject>) {
            self.controls().clear_history();
            self.ivars().recent_shown.borrow_mut().clear();
        }

        #[unsafe(method(toggleSetting:))]
        fn toggle_setting(&self, switch: &NSSwitch) {
            let Some(&toggle) = Toggle::ALL.get(switch.tag() as usize) else {
                return;
            };
            let on = switch.state() == NSControlStateValueOn;
            match toggle {
                Toggle::KeepHistory => self.controls().set_keep_history(on),
                _ => toggle.flag(self.controls()).store(on, Ordering::Relaxed),
            }
            save_config(|cfg| *toggle.field(cfg) = on);
        }

        #[unsafe(method(showSettings:))]
        fn show_settings_clicked(&self, _sender: Option<&AnyObject>) {
            self.show_settings();
        }

        #[unsafe(method(setupBack:))]
        fn setup_back(&self, _sender: Option<&AnyObject>) {
            if let Some(setup) = self.ivars().setup.get() {
                setup.go_to(setup.current().saturating_sub(1), self.controls());
            }
        }

        /// "Allow…" / "Open Settings" in a permissions checklist.
        #[unsafe(method(allowPermission:))]
        fn allow_permission(&self, button: &NSButton) {
            match button.tag() {
                0 => permissions::request_accessibility(),
                _ => permissions::request_microphone(),
            }
        }

        #[unsafe(method(checkPermissions:))]
        fn check_permissions(&self, _sender: Option<&AnyObject>) {
            self.refresh_permissions();
        }

        /// Records a second in the background, then reports whether sayit
        /// actually heard anything. The audio is discarded.
        #[unsafe(method(testMicrophone:))]
        fn test_microphone(&self, _sender: Option<&AnyObject>) {
            if self.ivars().testing_mic.replace(true) {
                return;
            }
            self.show_mic_test("Listening… say something.", NSColor::secondaryLabelColor());
            let device = self.controls().input_device();
            std::thread::spawn(move || {
                let heard = audio::Recorder::start(device.as_deref()).map(|rec| {
                    std::thread::sleep(Duration::from_millis(1500));
                    let samples = rec.stop();
                    samples.iter().fold(0.0f32, |peak, s| peak.max(s.abs()))
                });
                DispatchQueue::main().exec_async(move || {
                    with_actions(|a| a.ivars().testing_mic.set(false));
                    with_actions(|a| match heard {
                        Ok(peak) if peak > 0.02 => {
                            a.show_mic_test("✓ sayit can hear you.", NSColor::systemGreenColor())
                        }
                        Ok(_) => a.show_mic_test(
                            "✗ Only silence. Check the permission and the microphone choice.",
                            NSColor::systemRedColor(),
                        ),
                        Err(e) => a.show_mic_test(&format!("✗ {e:#}"), NSColor::systemRedColor()),
                    })
                });
            });
        }

        #[unsafe(method(showPermissions:))]
        fn show_permissions_clicked(&self, _sender: Option<&AnyObject>) {
            self.show_settings_tab(SettingsTab::Permissions);
        }

        #[unsafe(method(setupMicrophone:))]
        fn setup_microphone(&self, menu: &NSPopUpButton) {
            let choice = (menu.indexOfSelectedItem() > 0)
                .then(|| menu.titleOfSelectedItem().map(|t| t.to_string()))
                .flatten();
            *self.controls().input_device.lock().unwrap_or_else(|e| e.into_inner()) =
                choice.clone();
            save_config(|cfg| cfg.input_device = choice);
        }

        /// From setup or Settings › General.
        #[unsafe(method(chooseHotkey:))]
        fn choose_hotkey(&self, menu: &NSPopUpButton) {
            // Past the offered keys is a custom one from config.toml: keep it.
            if let Some(&key) = HOTKEYS.get(menu.indexOfSelectedItem() as usize) {
                save_config(|cfg| cfg.hotkey = key.to_string());
            }
            self.hotkey_changed();
        }

        #[unsafe(method(chooseMode:))]
        fn choose_mode(&self, control: &NSSegmentedControl) {
            let mode = if control.selectedSegment() == 1 {
                Mode::Toggle
            } else {
                Mode::Hold
            };
            save_config(|cfg| cfg.mode = mode);
            self.hotkey_changed();
        }

        #[unsafe(method(setupNext:))]
        fn setup_next(&self, _sender: Option<&AnyObject>) {
            let Some(setup) = self.ivars().setup.get() else {
                return;
            };
            if !setup.is_last() {
                setup.go_to(setup.current() + 1, self.controls());
                return;
            }
            save_config(|cfg| cfg.setup_complete = true);
            // orderOut, not close: closing the setup window quits sayit.
            setup.window.orderOut(None);
            super::finish_setup(self);
        }
    }

    unsafe impl NSObjectProtocol for Actions {}

    unsafe impl NSApplicationDelegate for Actions {
        #[unsafe(method(applicationWillTerminate:))]
        fn application_will_terminate(&self, _notification: &NSNotification) {
            self.stop_download();
        }

        /// A click on the Dock icon (when shown) opens Settings.
        #[unsafe(method(applicationShouldHandleReopen:hasVisibleWindows:))]
        fn application_should_handle_reopen(&self, _app: &NSApplication, _visible: bool) -> bool {
            if super::SETUP_DONE.load(Ordering::Relaxed) {
                self.show_settings();
            }
            true
        }
    }

    unsafe impl NSWindowDelegate for Actions {
        /// Closing the setup window quits sayit: nothing runs until setup is
        /// finished, and it starts again on the next launch. Closing Settings
        /// quits too if the user chose that.
        #[unsafe(method(windowWillClose:))]
        fn window_will_close(&self, notification: &NSNotification) {
            let closing = notification.object();
            let is = |window: &NSWindow| {
                closing.as_deref().is_some_and(|w| std::ptr::eq(w, window as &AnyObject))
            };
            let setup = self.ivars().setup.get().is_some_and(|s| is(&s.window));
            let settings = self.ivars().settings.get().is_some_and(|s| is(&s.window));
            if setup || (settings && self.ivars().on_close.get() == OnClose::Quit) {
                NSApplication::sharedApplication(self.mtm()).terminate(None);
            }
        }
    }

    unsafe impl NSMenuDelegate for Actions {
        /// Rebuilds a submenu each time it opens, so newly connected
        /// devices and newly downloaded models show up.
        #[unsafe(method(menuNeedsUpdate:))]
        fn menu_needs_update(&self, menu: &NSMenu) {
            match menu.title().to_string().as_str() {
                MODEL_MENU => fill_model_menu(self, menu),
                RECENT_MENU => {
                    let shown = self.controls().recent();
                    fill_recent_menu(self, menu, &shown);
                    *self.ivars().recent_shown.borrow_mut() = shown;
                }
                _ => fill_mic_menu(self, menu),
            }
        }
    }
);

impl Actions {
    pub fn new(mtm: MainThreadMarker, controls: Arc<Controls>) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(Ivars {
            controls,
            settings: OnceCell::new(),
            watching: Cell::new(false),
            testing_mic: Cell::new(false),
            setup: OnceCell::new(),
            model_rows: RefCell::new(Vec::new()),
            download: RefCell::new(None),
            progress: Cell::new(0.0),
            last_error: RefCell::new(None),
            on_close: Cell::new(OnClose::MenuBar),
            recent_shown: RefCell::new(Vec::new()),
        });
        // SAFETY: NSObject's designated initializer, called once on a fresh
        // allocation whose ivars are already set.
        unsafe { msg_send![super(this), init] }
    }

    pub fn controls(&self) -> &Controls {
        &self.ivars().controls
    }

    /// Switches the dictation service to `model` (it loads before the next
    /// take) and saves the choice.
    fn use_model(&self, model: ModelId) {
        *self
            .controls()
            .model
            .lock()
            .unwrap_or_else(|e| e.into_inner()) = model;
        save_config(|cfg| cfg.model = model);
        self.refresh_models();
    }

    pub fn show_settings(&self) {
        let settings = self.ivars().settings.get_or_init(|| {
            let (settings, rows) = settings::build(self.mtm(), self);
            settings
                .window
                .setDelegate(Some(ProtocolObject::from_ref(self)));
            self.ivars().model_rows.borrow_mut().extend(rows);
            settings
        });
        self.refresh_models();
        self.refresh_permissions();
        self.refresh_login();
        setup::bring_to_front(self.mtm(), &settings.window);
    }

    /// Remembers what closing Settings does, and shows or hides the Dock icon
    /// to match. The menu-bar icon stays either way.
    pub fn apply_on_close(&self, on_close: OnClose) {
        self.ivars().on_close.set(on_close);
        let app = NSApplication::sharedApplication(self.mtm());
        if on_close == OnClose::Dock {
            app.setActivationPolicy(NSApplicationActivationPolicy::Regular);
            // A development build has no bundle for macOS to take the icon from.
            if !paths::in_app_bundle() {
                // SAFETY: a valid NSImage (or nil for the default icon).
                unsafe { app.setApplicationIconImage(setup::app_icon_image().as_deref()) };
            }
        } else {
            app.setActivationPolicy(NSApplicationActivationPolicy::Accessory);
        }
        // Changing the policy can send the app to the background; keep an
        // open Settings window in front.
        if let Some(settings) = self.ivars().settings.get()
            && settings.window.isVisible()
        {
            setup::bring_to_front(self.mtm(), &settings.window);
        }
    }

    fn refresh_login(&self) {
        if let Some(settings) = self.ivars().settings.get() {
            settings.show_login(login::status());
        }
    }

    pub fn show_settings_tab(&self, tab: SettingsTab) {
        self.show_settings();
        if let Some(settings) = self.ivars().settings.get() {
            settings.select_tab(tab);
        }
    }

    /// Opens the setup assistant, or brings it back if it's already open.
    pub fn show_setup(&self) {
        let mut fresh = false;
        let setup = self.ivars().setup.get_or_init(|| {
            fresh = true;
            let cfg = Config::load().unwrap_or_default();
            let (setup, rows) = setup::build(self.mtm(), self, &cfg);
            setup
                .window
                .setDelegate(Some(ProtocolObject::from_ref(self)));
            self.ivars().model_rows.borrow_mut().extend(rows);
            setup
        });
        if fresh {
            setup.go_to(0, self.controls());
        }
        self.refresh_models();
        setup::bring_to_front(self.mtm(), &setup.window);
    }

    /// Checks both permissions every 2 seconds for the life of the app, so the
    /// checklists and the menu-bar warning follow changes made in System
    /// Settings. Each check is a cheap local query.
    pub fn watch_permissions(&self) {
        if self.ivars().watching.replace(true) {
            return;
        }
        std::thread::Builder::new()
            .name("sayit-permissions".into())
            .spawn(|| {
                loop {
                    DispatchQueue::main().exec_async(|| with_actions(|a| a.refresh_permissions()));
                    std::thread::sleep(Duration::from_secs(2));
                }
            })
            .ok();
    }

    /// Updates every checklist and the menu bar from what macOS reports now.
    pub fn refresh_permissions(&self) {
        if let Some(setup) = self.ivars().setup.get() {
            setup.refresh(self.controls());
        }
        let permitted = match self.ivars().settings.get() {
            Some(settings) => refresh_permissions(&settings.permission_rows),
            None => {
                permissions::accessibility() == permissions::Access::Allowed
                    && permissions::microphone() == permissions::Access::Allowed
            }
        };
        with_ui(|ui| ui.permitted.set(permitted));
    }

    fn show_mic_test(&self, text: &str, color: objc2::rc::Retained<NSColor>) {
        if let Some(settings) = self.ivars().settings.get() {
            settings.mic_test.setStringValue(&NSString::from_str(text));
            settings.mic_test.setTextColor(Some(&color));
        }
    }

    /// Applies a new hotkey or mode from config.toml: to the running listener,
    /// both pickers, the setup how-to and the menu-bar status line.
    fn hotkey_changed(&self) {
        let Ok(cfg) = Config::load() else {
            return;
        };
        self.controls().set_hotkey(&cfg.hotkey, cfg.mode);
        if let Some(setup) = self.ivars().setup.get() {
            setup.show_hotkey(&cfg);
        }
        if let Some(settings) = self.ivars().settings.get() {
            settings.hotkey.show(&cfg);
        }
        with_ui(|ui| ui.set_hotkey(&cfg.hotkey, cfg.mode));
    }

    /// Stops a model download when sayit quits, so it doesn't carry on in the
    /// background, and removes its partial files.
    fn stop_download(&self) {
        if let Some((model, download)) = self.ivars().download.borrow_mut().take()
            && download.cancel_and_wait(Duration::from_secs(2))
            && let Err(e) = models::delete(model)
        {
            eprintln!("could not remove the partial download: {e:#}");
        }
    }

    fn download_progress(&self, fraction: f64) {
        self.ivars().progress.set(fraction);
        self.refresh_models();
    }

    fn download_finished(&self, model: ModelId, finished: Finished) {
        self.ivars().download.borrow_mut().take();
        match finished {
            // With nothing usable yet (e.g. first run), use the new model.
            Finished::Installed if !self.controls().model().is_installed() => self.use_model(model),
            Finished::Installed => {}
            // Cancel means "leave nothing behind": remove the partial files.
            Finished::Cancelled => {
                if let Err(e) = models::delete(model) {
                    *self.ivars().last_error.borrow_mut() = Some((model, format!("{e:#}")));
                }
            }
            Finished::Failed(e) => *self.ivars().last_error.borrow_mut() = Some((model, e)),
        }
        self.refresh_models();
    }

    fn refresh_models(&self) {
        let rows = self.ivars().model_rows.borrow();
        let downloading = self.ivars().download.borrow().as_ref().map(|(m, _)| *m);
        let last_error = self.ivars().last_error.borrow();
        let state = ModelsState {
            current: self.controls().model(),
            downloading: downloading.map(|m| (m, self.ivars().progress.get())),
            error: last_error.as_ref(),
        };
        settings::refresh_models(&rows, &state);
        if let Some(setup) = self.ivars().setup.get() {
            setup.refresh(self.controls());
        }
    }
}
