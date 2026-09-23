//! `Actions` receives every click from the menus and the Settings window.
//! AppKit calls back by selector, so this is an Objective-C class defined in
//! Rust; each method hands off to plain Rust.

use std::cell::{Cell, OnceCell, RefCell};
use std::sync::Arc;
use std::sync::atomic::Ordering;

use dispatch2::DispatchQueue;
use objc2::rc::Retained;
use objc2::runtime::{AnyObject, NSObject};
use objc2::{DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send};
use objc2_app_kit::{
    NSAlert, NSAlertFirstButtonReturn, NSButton, NSControlStateValueOn, NSMenu, NSMenuDelegate,
    NSMenuItem, NSSwitch, NSWindow,
};
use objc2_foundation::{NSObjectProtocol, NSString};

use super::settings::{self, ModelRow, ModelsState, Toggle};
use super::setup::{self, Setup};
use super::widgets::check_state;
use super::{
    DEFAULT_MIC_TAG, MODEL_MENU, fill_mic_menu, fill_model_menu, save_config, with_actions, with_ui,
};
use crate::daemon::Controls;
use crate::models::{self, Download, Finished};
use crate::stt::ModelId;

pub struct Ivars {
    controls: Arc<Controls>,
    /// Shown in the setup assistant's how-to, e.g. "Right Option".
    hotkey: String,
    settings: OnceCell<Retained<NSWindow>>,
    setup: OnceCell<Setup>,
    /// Model cards in Settings and in the setup assistant, kept in sync.
    model_rows: RefCell<Vec<ModelRow>>,
    /// At most one model downloads at a time.
    download: RefCell<Option<(ModelId, Download)>>,
    progress: Cell<f64>,
    last_error: RefCell<Option<(ModelId, String)>>,
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

        #[unsafe(method(toggleSetting:))]
        fn toggle_setting(&self, switch: &NSSwitch) {
            let Some(&toggle) = Toggle::ALL.get(switch.tag() as usize) else {
                return;
            };
            let on = switch.state() == NSControlStateValueOn;
            toggle.flag(self.controls()).store(on, Ordering::Relaxed);
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
            setup.window.orderOut(None);
            super::finish_setup();
        }
    }

    unsafe impl NSObjectProtocol for Actions {}

    unsafe impl NSMenuDelegate for Actions {
        /// Rebuilds a submenu each time it opens, so newly connected
        /// devices and newly downloaded models show up.
        #[unsafe(method(menuNeedsUpdate:))]
        fn menu_needs_update(&self, menu: &NSMenu) {
            if menu.title().to_string() == MODEL_MENU {
                fill_model_menu(self, menu);
            } else {
                fill_mic_menu(self, menu);
            }
        }
    }
);

impl Actions {
    pub fn new(mtm: MainThreadMarker, controls: Arc<Controls>, hotkey: String) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(Ivars {
            controls,
            hotkey,
            settings: OnceCell::new(),
            setup: OnceCell::new(),
            model_rows: RefCell::new(Vec::new()),
            download: RefCell::new(None),
            progress: Cell::new(0.0),
            last_error: RefCell::new(None),
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
        let window = self.ivars().settings.get_or_init(|| {
            let (window, rows) = settings::build(self.mtm(), self);
            self.ivars().model_rows.borrow_mut().extend(rows);
            window
        });
        self.refresh_models();
        settings::show(self.mtm(), window);
    }

    pub fn show_setup(&self) {
        let setup = self.ivars().setup.get_or_init(|| {
            let (setup, rows) = setup::build(self.mtm(), self, &self.ivars().hotkey);
            self.ivars().model_rows.borrow_mut().extend(rows);
            setup
        });
        setup.go_to(0, self.controls());
        self.refresh_models();
        setup::show(self.mtm(), setup);
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
