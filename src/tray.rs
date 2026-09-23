//! Menu-bar app: a status icon showing what sayit is doing, with Pause,
//! microphone and model pickers, and Quit. The dictation service runs on background threads (see daemon.rs);
//! AppKit owns the main thread.

use anyhow::Result;

use crate::config::Config;

pub use platform::run;

#[cfg(target_os = "macos")]
mod platform {
    use std::cell::{Cell, OnceCell, RefCell};
    use std::sync::Arc;
    use std::sync::atomic::Ordering;
    use std::time::Duration;

    use dispatch2::DispatchQueue;
    use objc2::rc::Retained;
    use objc2::runtime::{AnyObject, NSObject, ProtocolObject};
    use objc2::{define_class, msg_send, sel, DefinedClass, MainThreadMarker, MainThreadOnly};
    use objc2_app_kit::{
        NSApplication, NSApplicationActivationPolicy, NSControlStateValueOff, NSControlStateValueOn,
        NSImage, NSMenu, NSMenuDelegate, NSMenuItem, NSStatusBar, NSStatusItem,
        NSVariableStatusItemLength,
    };
    use objc2_foundation::{NSObjectProtocol, NSString};

    use super::*;
    use crate::audio;
    use crate::daemon::{self, Controls, Status};
    use crate::stt::ModelId;

    const MIC_MENU: &str = "Microphone";
    const MODEL_MENU: &str = "Model";

    /// Tag of the "System Default" microphone item; device items use tag 0
    /// and their title is the device name.
    const DEFAULT_MIC_TAG: isize = 1;

    /// What the icon and the status line show.
    #[derive(Clone, Copy, PartialEq)]
    enum State {
        Loading,
        Ready(Status),
        NeedsAccessibility,
        Failed,
    }

    /// UI objects, only touched on the main thread.
    struct Ui {
        item: Retained<NSStatusItem>,
        status_line: Retained<NSMenuItem>,
        hotkey: String,
        state: Cell<State>,
        paused: Cell<bool>,
        error: RefCell<String>,
    }

    thread_local! {
        static UI: OnceCell<Ui> = const { OnceCell::new() };
    }

    struct TargetIvars {
        controls: Arc<Controls>,
    }

    define_class!(
        // SAFETY: NSObject has no subclassing requirements; MenuTarget adds no
        // Drop impl and its methods run on the main thread.
        #[unsafe(super(NSObject))]
        #[thread_kind = MainThreadOnly]
        #[name = "SayitMenuTarget"]
        #[ivars = TargetIvars]
        struct MenuTarget;

        impl MenuTarget {
            #[unsafe(method(togglePause:))]
            fn toggle_pause(&self, item: &NSMenuItem) {
                let controls = &self.ivars().controls;
                let paused = !controls.paused.load(Ordering::Relaxed);
                controls.paused.store(paused, Ordering::Relaxed);
                item.setState(if paused { NSControlStateValueOn } else { NSControlStateValueOff });
                UI.with(|ui| {
                    if let Some(ui) = ui.get() {
                        ui.paused.set(paused);
                        render(ui);
                    }
                });
            }

            #[unsafe(method(selectMic:))]
            fn select_mic(&self, item: &NSMenuItem) {
                let choice = (item.tag() != DEFAULT_MIC_TAG).then(|| item.title().to_string());
                *self.ivars().controls.input_device.lock().unwrap_or_else(|e| e.into_inner()) =
                    choice.clone();
                save_config(|cfg| cfg.input_device = choice);
            }

            #[unsafe(method(selectModel:))]
            fn select_model(&self, item: &NSMenuItem) {
                let Some(&model) = ModelId::ALL.get(item.tag() as usize) else { return };
                *self.ivars().controls.model.lock().unwrap_or_else(|e| e.into_inner()) = model;
                save_config(|cfg| cfg.model = model);
            }
        }

        unsafe impl NSObjectProtocol for MenuTarget {}

        unsafe impl NSMenuDelegate for MenuTarget {
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

    impl MenuTarget {
        fn new(mtm: MainThreadMarker, controls: Arc<Controls>) -> Retained<Self> {
            let this = Self::alloc(mtm).set_ivars(TargetIvars { controls });
            // SAFETY: NSObject's designated initializer, called once on a fresh
            // allocation whose ivars are already set.
            unsafe { msg_send![super(this), init] }
        }
    }

    fn render(ui: &Ui) {
        let (symbol, text) = match (ui.state.get(), ui.paused.get()) {
            (State::Loading, _) => ("hourglass", "Loading model…".to_string()),
            (State::NeedsAccessibility, _) => (
                "exclamationmark.triangle",
                "Waiting for Accessibility permission (System Settings › Privacy & Security)".into(),
            ),
            (State::Failed, _) => ("exclamationmark.triangle", format!("Stopped: {}", ui.error.borrow())),
            (State::Ready(_), true) => ("mic.slash", "Paused".into()),
            (State::Ready(Status::Idle), false) => ("mic", format!("Ready: hold {} to dictate", ui.hotkey)),
            (State::Ready(Status::Recording), false) => ("mic.fill", "Listening…".into()),
            (State::Ready(Status::Transcribing), false) => ("waveform", "Transcribing…".into()),
        };
        ui.status_line.setTitle(&NSString::from_str(&text));
        let mtm = MainThreadMarker::new().expect("render runs on the main thread");
        if let Some(button) = ui.item.button(mtm) {
            let image = NSImage::imageWithSystemSymbolName_accessibilityDescription(
                &NSString::from_str(symbol),
                Some(&NSString::from_str(&text)),
            );
            if let Some(image) = &image {
                image.setTemplate(true); // follows light/dark menu bar
            }
            button.setImage(image.as_deref());
            button.setToolTip(Some(&NSString::from_str(&format!("sayit: {text}"))));
        }
    }

    /// Sets the state from any thread; the UI update runs on the main thread.
    fn set_state(state: State) {
        DispatchQueue::main().exec_async(move || {
            UI.with(|ui| {
                if let Some(ui) = ui.get() {
                    ui.state.set(state);
                    render(ui);
                }
            })
        });
    }

    fn set_failed(message: String) {
        DispatchQueue::main().exec_async(move || {
            UI.with(|ui| {
                if let Some(ui) = ui.get() {
                    *ui.error.borrow_mut() = message;
                    ui.state.set(State::Failed);
                    render(ui);
                }
            })
        });
    }

    fn fill_mic_menu(target: &MenuTarget, menu: &NSMenu) {
        let mtm = target.mtm();
        let current = target.ivars().controls.input_device();
        let target: &AnyObject = target;
        let names = audio::input_device_names();
        menu.removeAllItems();

        let add = |title: &str, tag: isize, checked: bool, enabled: bool| {
            let action = enabled.then(|| sel!(selectMic:));
            let item = menu_item(mtm, title, action, "");
            item.setTag(tag);
            item.setState(if checked { NSControlStateValueOn } else { NSControlStateValueOff });
            if enabled {
                // SAFETY: the target outlives the menu (see `run`).
                unsafe { item.setTarget(Some(target)) };
            }
            menu.addItem(&item);
        };
        add("System Default", DEFAULT_MIC_TAG, current.is_none(), true);
        menu.addItem(&NSMenuItem::separatorItem(mtm));
        for name in &names {
            add(name, 0, current.as_deref() == Some(name.as_str()), true);
        }
        if let Some(cur) = current.filter(|c| !names.contains(c)) {
            // Chosen device is unplugged: show it, but takes use the default.
            add(&format!("{cur} (not connected)"), 0, true, false);
        }
    }

    fn fill_model_menu(target: &MenuTarget, menu: &NSMenu) {
        let mtm = target.mtm();
        let current = target.ivars().controls.model();
        let target: &AnyObject = target;
        menu.removeAllItems();
        for (i, model) in ModelId::ALL.into_iter().enumerate() {
            let installed = model.is_installed();
            let title = if installed {
                model.label().to_string()
            } else {
                format!("{} (not downloaded)", model.label())
            };
            let item = menu_item(mtm, &title, installed.then(|| sel!(selectModel:)), "");
            item.setTag(i as isize);
            if model == current {
                item.setState(NSControlStateValueOn);
            }
            if installed {
                // SAFETY: the target outlives the menu (see `run`).
                unsafe { item.setTarget(Some(target)) };
            }
            menu.addItem(&item);
        }
    }

    /// Changes one setting in config.toml. Re-reads the file so edits made
    /// while sayit runs aren't lost, and never overwrites a file it couldn't parse.
    fn save_config(change: impl FnOnce(&mut Config)) {
        let saved = Config::load().and_then(|mut cfg| {
            change(&mut cfg);
            cfg.save()
        });
        if let Err(e) = saved {
            eprintln!("could not save setting: {e:#}");
        }
    }

    fn menu_item(mtm: MainThreadMarker, title: &str, action: Option<objc2::runtime::Sel>, key: &str) -> Retained<NSMenuItem> {
        // SAFETY: `action` is either None or a selector implemented by the
        // item's target (MenuTarget) or by NSApplication (terminate:).
        unsafe {
            NSMenuItem::initWithTitle_action_keyEquivalent(
                NSMenuItem::alloc(mtm),
                &NSString::from_str(title),
                action,
                &NSString::from_str(key),
            )
        }
    }

    pub fn run(cfg: Config, verbose: bool) -> Result<()> {
        let mtm = MainThreadMarker::new().ok_or_else(|| anyhow::anyhow!("must run on the main thread"))?;
        let app = NSApplication::sharedApplication(mtm);
        // Menu-bar only: no Dock icon, no app switcher entry.
        app.setActivationPolicy(NSApplicationActivationPolicy::Accessory);

        let controls = Controls::new(&cfg);
        let target = MenuTarget::new(mtm, controls.clone());

        let menu = NSMenu::new(mtm);
        // No action = disabled, so it reads as a label.
        let status_line = menu_item(mtm, "", None, "");
        menu.addItem(&status_line);
        menu.addItem(&NSMenuItem::separatorItem(mtm));
        let pause = menu_item(mtm, "Pause Dictation", Some(sel!(togglePause:)), "");
        // SAFETY: `target` is kept alive until the app exits (see end of fn).
        unsafe { pause.setTarget(Some(&target as &AnyObject)) };
        menu.addItem(&pause);
        // The Model menu only appears once there is a choice to make.
        let submenus: &[&str] = if ModelId::ALL.len() > 1 { &[MIC_MENU, MODEL_MENU] } else { &[MIC_MENU] };
        for &title in submenus {
            let item = menu_item(mtm, title, None, "");
            let submenu = NSMenu::initWithTitle(NSMenu::alloc(mtm), &NSString::from_str(title));
            submenu.setDelegate(Some(ProtocolObject::from_ref(&*target)));
            item.setSubmenu(Some(&submenu));
            menu.addItem(&item);
        }
        menu.addItem(&NSMenuItem::separatorItem(mtm));
        // terminate: goes to NSApplication through the responder chain.
        menu.addItem(&menu_item(mtm, "Quit sayit", Some(sel!(terminate:)), "q"));

        let item = NSStatusBar::systemStatusBar().statusItemWithLength(NSVariableStatusItemLength);
        item.setMenu(Some(&menu));

        // Shows the system prompt once if needed; the start thread then waits.
        let trusted = daemon::accessibility_trusted(true);
        UI.with(|ui| {
            let ui = ui.get_or_init(|| Ui {
                item,
                status_line,
                hotkey: cfg.hotkey.clone(),
                state: Cell::new(if trusted { State::Loading } else { State::NeedsAccessibility }),
                paused: Cell::new(false),
                error: RefCell::new(String::new()),
            });
            render(ui);
        });

        // Model loading takes ~1 s; keep the main thread responsive.
        std::thread::Builder::new().name("sayit-start".into()).spawn(move || {
            if !trusted {
                while !daemon::accessibility_trusted(false) {
                    std::thread::sleep(Duration::from_secs(1));
                }
                set_state(State::Loading);
            }
            let result = daemon::start(cfg, verbose, controls, |s| set_state(State::Ready(s)))
                .and_then(|d| {
                    set_state(State::Ready(Status::Idle));
                    d.wait()
                });
            if let Err(e) = result {
                eprintln!("{e:#}");
                // Outermost context only: it's the actionable part and has no paths.
                set_failed(e.to_string());
            }
        })?;

        app.run();
        drop(target);
        Ok(())
    }
}

#[cfg(not(target_os = "macos"))]
mod platform {
    use super::*;

    /// Placeholder until the Linux tray (StatusNotifierItem) is built.
    pub fn run(_cfg: Config, _verbose: bool) -> Result<()> {
        anyhow::bail!("the menu-bar app is macOS-only for now; use `sayit run` on Linux")
    }
}
