//! `Actions` receives every click from the menus and the Settings window.
//! AppKit calls back by selector, so this is an Objective-C class defined in
//! Rust; each method hands off to plain Rust.

use std::cell::OnceCell;
use std::sync::Arc;
use std::sync::atomic::Ordering;

use objc2::rc::Retained;
use objc2::runtime::{AnyObject, NSObject};
use objc2::{DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send};
use objc2_app_kit::{
    NSButton, NSControlStateValueOn, NSMenu, NSMenuDelegate, NSMenuItem, NSWindow,
};
use objc2_foundation::NSObjectProtocol;

use super::settings::{self, Toggle};
use super::widgets::check_state;
use super::{DEFAULT_MIC_TAG, MODEL_MENU, fill_mic_menu, fill_model_menu, save_config, with_ui};
use crate::daemon::Controls;
use crate::stt::ModelId;

pub struct Ivars {
    controls: Arc<Controls>,
    settings: OnceCell<Retained<NSWindow>>,
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
            let Some(&model) = ModelId::ALL.get(item.tag() as usize) else {
                return;
            };
            *self.controls().model.lock().unwrap_or_else(|e| e.into_inner()) = model;
            save_config(|cfg| cfg.model = model);
        }

        #[unsafe(method(toggleSetting:))]
        fn toggle_setting(&self, checkbox: &NSButton) {
            let Some(&toggle) = Toggle::ALL.get(checkbox.tag() as usize) else {
                return;
            };
            let on = checkbox.state() == NSControlStateValueOn;
            toggle.flag(self.controls()).store(on, Ordering::Relaxed);
            save_config(|cfg| *toggle.field(cfg) = on);
        }

        #[unsafe(method(showSettings:))]
        fn show_settings(&self, _sender: Option<&AnyObject>) {
            let window = self.ivars().settings.get_or_init(|| settings::build(self.mtm(), self));
            settings::show(self.mtm(), window);
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
    pub fn new(mtm: MainThreadMarker, controls: Arc<Controls>) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(Ivars {
            controls,
            settings: OnceCell::new(),
        });
        // SAFETY: NSObject's designated initializer, called once on a fresh
        // allocation whose ivars are already set.
        unsafe { msg_send![super(this), init] }
    }

    pub fn controls(&self) -> &Controls {
        &self.ivars().controls
    }
}
