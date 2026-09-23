//! macOS menu-bar app: a status icon showing what sayit is doing, with Pause,
//! microphone and model pickers, Settings and Quit. AppKit owns the main
//! thread; the dictation service runs on background threads (see daemon.rs).

mod actions;
mod permissions;
mod settings;
mod setup;
mod widgets;

use std::cell::{Cell, OnceCell, RefCell};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use anyhow::Result;
use dispatch2::DispatchQueue;
use objc2::rc::Retained;
use objc2::runtime::{AnyObject, ProtocolObject, Sel};
use objc2::{MainThreadMarker, MainThreadOnly, sel};
use objc2_app_kit::{
    NSApplication, NSApplicationActivationPolicy, NSImage, NSMenu, NSMenuItem, NSStatusBar,
    NSStatusItem, NSVariableStatusItemLength,
};
use objc2_foundation::NSString;

use actions::Actions;
use settings::SettingsTab;
use widgets::{check_state, menu_item};

use crate::audio;
use crate::config::Config;
use crate::daemon::{self, Controls, Status};
use crate::stt::ModelId;

const MIC_MENU: &str = "Microphone";
const MODEL_MENU: &str = "Model";

/// Tag of the "System Default" microphone item; device items use tag 0 and
/// their title is the device name.
const DEFAULT_MIC_TAG: isize = 1;

/// What the icon and the status line show.
#[derive(Clone, Copy, PartialEq)]
enum State {
    Loading,
    Ready(Status),
    NeedsAccessibility,
    NeedsModel,
    Failed,
}

/// Menu-bar objects, only touched on the main thread.
struct Ui {
    item: Retained<NSStatusItem>,
    status_line: Retained<NSMenuItem>,
    /// Shown when macOS reports a permission missing.
    fix_permissions: Retained<NSMenuItem>,
    permitted: Cell<bool>,
    hotkey: String,
    state: Cell<State>,
    paused: Cell<bool>,
    error: RefCell<String>,
}

thread_local! {
    static UI: OnceCell<Ui> = const { OnceCell::new() };
    static ACTIONS: OnceCell<Retained<Actions>> = const { OnceCell::new() };
}

/// Set once the setup assistant is finished (or was finished on an earlier
/// launch). Dictation, and its Accessibility prompt, waits for it.
static SETUP_DONE: AtomicBool = AtomicBool::new(false);

/// Adds the menu-bar icon and lets dictation start. The icon only appears
/// once setup is done; until then the setup window is the whole app.
fn finish_setup(actions: &Actions) {
    // Setup may have changed the hotkey.
    let hotkey = Config::load().unwrap_or_default().hotkey;
    let (item, status_line, fix_permissions) = status_item(actions.mtm(), actions);
    UI.with(|ui| {
        let ui = ui.get_or_init(|| Ui {
            item,
            status_line,
            fix_permissions,
            permitted: Cell::new(true),
            hotkey: hotkey_name(&hotkey),
            state: Cell::new(State::Loading),
            paused: Cell::new(false),
            error: RefCell::new(String::new()),
        });
        render(ui);
    });
    SETUP_DONE.store(true, Ordering::Relaxed);
}

/// The configured hotkey as people know it, e.g. "OptRight" -> "Right Option".
fn hotkey_name(hotkey: &str) -> String {
    fn side(key: &str) -> &str {
        match key {
            "Opt" => "Option",
            "Cmd" => "Command",
            "Ctrl" => "Control",
            other => other,
        }
    }
    for (suffix, prefix) in [("Right", "Right "), ("Left", "Left ")] {
        if let Some(key) = hotkey.strip_suffix(suffix) {
            return format!("{prefix}{}", side(key));
        }
    }
    hotkey.replace('+', " + ")
}

/// Runs `f` with the app's `Actions`. Main thread only; background threads
/// get here through `DispatchQueue::main()`.
fn with_actions(f: impl FnOnce(&Actions)) {
    ACTIONS.with(|a| {
        if let Some(actions) = a.get() {
            f(actions);
        }
    });
}

pub fn run(cfg: Config, verbose: bool) -> Result<()> {
    let mtm = MainThreadMarker::new()
        .ok_or_else(|| anyhow::anyhow!("the menu-bar app must run on the main thread"))?;
    let app = NSApplication::sharedApplication(mtm);
    // Menu-bar only: no Dock icon, no app switcher entry.
    app.setActivationPolicy(NSApplicationActivationPolicy::Accessory);

    let controls = Controls::new(&cfg);
    let actions = Actions::new(mtm, controls.clone());
    app.setDelegate(Some(ProtocolObject::from_ref(&*actions)));
    app.setMainMenu(Some(&main_menu(mtm, &actions)));
    if cfg.setup_complete {
        finish_setup(&actions);
    } else {
        actions.show_setup();
    }
    actions.watch_permissions();
    // Kept here for the life of the app: the app, menus and windows hold only
    // weak references to it.
    ACTIONS.with(|a| a.set(actions).ok());

    // Model loading takes ~1 s; keep the main thread responsive.
    std::thread::Builder::new()
        .name("sayit-start".into())
        .spawn(move || {
            while !SETUP_DONE.load(Ordering::Relaxed) {
                std::thread::sleep(Duration::from_millis(250));
            }
            // Setup may have changed the hotkey or mode.
            let cfg = Config::load().unwrap_or(cfg);
            // Shows the system prompt once if needed, then waits for it.
            if !daemon::accessibility_trusted(true) {
                set_state(State::NeedsAccessibility);
                while !daemon::accessibility_trusted(false) {
                    std::thread::sleep(Duration::from_secs(1));
                }
                set_state(State::Loading);
            }
            if !controls.model().is_installed() {
                // First run, or the chosen model was deleted: open Settings
                // so the user can download one, and wait for it.
                set_state(State::NeedsModel);
                DispatchQueue::main()
                    .exec_async(|| with_actions(|a| a.show_settings_tab(SettingsTab::Models)));
                while !controls.model().is_installed() {
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
    Ok(())
}

/// The menu shown from the menu-bar icon.
fn status_item(
    mtm: MainThreadMarker,
    actions: &Actions,
) -> (
    Retained<NSStatusItem>,
    Retained<NSMenuItem>,
    Retained<NSMenuItem>,
) {
    let target: &AnyObject = actions;
    let with_target = |item: &NSMenuItem| {
        // SAFETY: `actions` outlives every menu (see `run`).
        unsafe { item.setTarget(Some(target)) };
    };

    let menu = NSMenu::new(mtm);
    // No action = disabled, so it reads as a label.
    let status_line = menu_item(mtm, "", None, "");
    menu.addItem(&status_line);
    let fix_permissions = menu_item(mtm, "Fix Permissions…", Some(sel!(showPermissions:)), "");
    with_target(&fix_permissions);
    fix_permissions.setHidden(true);
    menu.addItem(&fix_permissions);
    menu.addItem(&NSMenuItem::separatorItem(mtm));

    let pause = menu_item(mtm, "Pause Dictation", Some(sel!(togglePause:)), "");
    with_target(&pause);
    menu.addItem(&pause);
    // The Model menu only appears once there is a choice to make.
    let submenus = if ModelId::ALL.len() > 1 {
        &[MIC_MENU, MODEL_MENU][..]
    } else {
        &[MIC_MENU]
    };
    for &title in submenus {
        let item = menu_item(mtm, title, None, "");
        let submenu = NSMenu::initWithTitle(NSMenu::alloc(mtm), &NSString::from_str(title));
        submenu.setDelegate(Some(ProtocolObject::from_ref(actions)));
        item.setSubmenu(Some(&submenu));
        menu.addItem(&item);
    }
    menu.addItem(&NSMenuItem::separatorItem(mtm));

    let settings = menu_item(mtm, "Settings…", Some(sel!(showSettings:)), ",");
    with_target(&settings);
    menu.addItem(&settings);
    // terminate: goes to NSApplication through the responder chain.
    menu.addItem(&menu_item(mtm, "Quit sayit", Some(sel!(terminate:)), "q"));

    let item = NSStatusBar::systemStatusBar().statusItemWithLength(NSVariableStatusItemLength);
    item.setMenu(Some(&menu));
    (item, status_line, fix_permissions)
}

/// A menu-bar-only app never shows its main menu, but AppKit still uses it
/// for keyboard shortcuts: without it, ⌘W, ⌘Q and copy/paste do nothing in
/// the Settings window.
fn main_menu(mtm: MainThreadMarker, actions: &Actions) -> Retained<NSMenu> {
    let submenu = |title: &str, items: &[(&str, Sel, &str)]| {
        let menu = NSMenu::initWithTitle(NSMenu::alloc(mtm), &NSString::from_str(title));
        for &(title, action, key) in items {
            menu.addItem(&menu_item(mtm, title, Some(action), key));
        }
        let item = menu_item(mtm, title, None, "");
        item.setSubmenu(Some(&menu));
        item
    };

    let app_menu = submenu("sayit", &[("Quit sayit", sel!(terminate:), "q")]);
    let settings = menu_item(mtm, "Settings…", Some(sel!(showSettings:)), ",");
    // SAFETY: `actions` outlives the menu (see `run`).
    unsafe { settings.setTarget(Some(actions as &AnyObject)) };
    app_menu
        .submenu()
        .expect("just set")
        .insertItem_atIndex(&settings, 0);

    let edit = submenu(
        "Edit",
        &[
            ("Undo", sel!(undo:), "z"),
            ("Cut", sel!(cut:), "x"),
            ("Copy", sel!(copy:), "c"),
            ("Paste", sel!(paste:), "v"),
            ("Select All", sel!(selectAll:), "a"),
        ],
    );
    let window = submenu("Window", &[("Close", sel!(performClose:), "w")]);

    let main = NSMenu::new(mtm);
    for item in [app_menu, edit, window] {
        main.addItem(&item);
    }
    main
}

fn render(ui: &Ui) {
    ui.fix_permissions.setHidden(ui.permitted.get());
    let missing = !ui.permitted.get() && matches!(ui.state.get(), State::Ready(_));
    let (symbol, text) = match (ui.state.get(), ui.paused.get()) {
        _ if missing => (
            "exclamationmark.triangle",
            "A permission is missing: choose Fix Permissions…".to_string(),
        ),
        (State::Loading, _) => ("hourglass", "Loading model…".to_string()),
        (State::NeedsAccessibility, _) => (
            "exclamationmark.triangle",
            "Waiting for Accessibility permission (System Settings › Privacy & Security)".into(),
        ),
        (State::NeedsModel, _) => (
            "arrow.down.circle",
            "No speech model yet: download one in Settings".into(),
        ),
        (State::Failed, _) => (
            "exclamationmark.triangle",
            format!("Stopped: {}", ui.error.borrow()),
        ),
        (State::Ready(_), true) => ("mic.slash", "Paused".into()),
        (State::Ready(Status::Idle), false) => {
            ("mic", format!("Ready: hold {} to dictate", ui.hotkey))
        }
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

/// Changes the menu-bar state and redraws it. Main thread only.
fn with_ui(change: impl FnOnce(&Ui)) {
    UI.with(|ui| {
        if let Some(ui) = ui.get() {
            change(ui);
            render(ui);
        }
    });
}

/// Sets the state from any thread; the UI update runs on the main thread.
fn set_state(state: State) {
    DispatchQueue::main().exec_async(move || with_ui(|ui| ui.state.set(state)));
}

fn set_failed(message: String) {
    DispatchQueue::main().exec_async(move || {
        with_ui(|ui| {
            *ui.error.borrow_mut() = message;
            ui.state.set(State::Failed);
        })
    });
}

/// Adds a checkable item to a picker submenu. Items without an action are
/// shown greyed out.
fn add_choice(
    menu: &NSMenu,
    actions: &Actions,
    title: &str,
    action: Option<Sel>,
    tag: isize,
    checked: bool,
) {
    let item = menu_item(actions.mtm(), title, action, "");
    item.setTag(tag);
    item.setState(check_state(checked));
    if action.is_some() {
        // SAFETY: `actions` outlives every menu (see `run`).
        unsafe { item.setTarget(Some(actions as &AnyObject)) };
    }
    menu.addItem(&item);
}

fn fill_mic_menu(actions: &Actions, menu: &NSMenu) {
    let current = actions.controls().input_device();
    let names = audio::input_device_names();
    let select = Some(sel!(selectMic:));
    menu.removeAllItems();
    add_choice(
        menu,
        actions,
        "System Default",
        select,
        DEFAULT_MIC_TAG,
        current.is_none(),
    );
    menu.addItem(&NSMenuItem::separatorItem(actions.mtm()));
    for name in &names {
        let checked = current.as_ref() == Some(name);
        add_choice(menu, actions, name, select, 0, checked);
    }
    if let Some(cur) = current.filter(|c| !names.contains(c)) {
        // The chosen device is unplugged: keep showing it, but takes use the default.
        let title = format!("{cur} (not connected)");
        add_choice(menu, actions, &title, None, 0, true);
    }
}

fn fill_model_menu(actions: &Actions, menu: &NSMenu) {
    let current = actions.controls().model();
    menu.removeAllItems();
    for (i, model) in ModelId::ALL.into_iter().enumerate() {
        let (title, action) = if model.is_installed() {
            (model.label().to_string(), Some(sel!(selectModel:)))
        } else {
            (format!("{} (not downloaded)", model.label()), None)
        };
        add_choice(menu, actions, &title, action, i as isize, model == current);
    }
}

/// Changes one setting in config.toml. Re-reads the file so edits made while
/// sayit runs aren't lost, and never overwrites a file it couldn't parse.
fn save_config(change: impl FnOnce(&mut Config)) {
    let saved = Config::load().and_then(|mut cfg| {
        change(&mut cfg);
        cfg.save()
    });
    if let Err(e) = saved {
        eprintln!("could not save setting: {e:#}");
    }
}
