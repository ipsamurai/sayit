//! The setup assistant shown on first launch. Each page is a centred icon,
//! title and one-line subtitle above a card of controls. Finishing it saves
//! `setup_complete` and starts dictation. Closing or quitting partway is fine:
//! "Continue Setup…" in the menu bar reopens it, and so does the next launch.

use std::cell::Cell;
use std::sync::atomic::Ordering;

use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::{AllocAnyThread, MainThreadMarker, MainThreadOnly, sel};
use objc2_app_kit::{
    NSApplication, NSBackingStoreType, NSBox, NSButton, NSColor, NSControlSize, NSFont, NSImage,
    NSImageView, NSLayoutAttribute, NSPopUpButton, NSStackView, NSStackViewDistribution,
    NSStackViewGravity, NSTabView, NSTabViewItem, NSTabViewType, NSTextField,
    NSUserInterfaceLayoutOrientation, NSView, NSWindow, NSWindowStyleMask,
};
use objc2_foundation::{NSEdgeInsets, NSPoint, NSRect, NSSize, NSString};

use super::actions::Actions;
use super::hotkey_name;
use super::permissions::{self, Access};
use super::settings::{ModelRow, PANE_WIDTH, Toggle, model_card};
use super::widgets::{
    button, card_row, centered, fixed_height, fixed_width, icon_badge, label, note, popup,
    rows_card, segmented, semibold, separator, set_symbol, spacer, stack, switch, symbol,
};
use crate::audio;
use crate::config::{Config, Mode};
use crate::daemon::Controls;
use crate::stt::ModelId;

/// The assistant's pages, in order.
#[derive(Clone, Copy, PartialEq)]
enum Page {
    Welcome,
    Model,
    Permissions,
    Microphone,
    Hotkey,
    Text,
    Done,
}

const PAGES: [Page; 7] = [
    Page::Welcome,
    Page::Model,
    Page::Permissions,
    Page::Microphone,
    Page::Hotkey,
    Page::Text,
    Page::Done,
];

/// Hotkeys offered in setup (config names). Any other key can still be set
/// in config.toml.
pub const HOTKEYS: [&str; 5] = ["OptRight", "OptLeft", "CmdRight", "CtrlRight", "Fn"];

/// Width inside a card.
const CARD_INNER: f64 = PANE_WIDTH - 28.0;

/// One line of the permissions checklist: a green tick or red cross, and
/// either "Allowed" or a button to fix it.
pub struct PermissionRow {
    status: Retained<NSImageView>,
    allowed: Retained<NSTextField>,
    button: Retained<NSButton>,
}

impl PermissionRow {
    pub fn show(&self, access: Access) {
        let (icon, color) = match access {
            Access::Allowed => ("checkmark.circle.fill", NSColor::systemGreenColor()),
            _ => ("xmark.circle.fill", NSColor::systemRedColor()),
        };
        set_symbol(&self.status, icon, 18.0, &color);
        self.allowed.setHidden(access != Access::Allowed);
        self.button.setHidden(access == Access::Allowed);
        let title = if access == Access::Denied {
            "Open Settings"
        } else {
            "Allow…"
        };
        self.button.setTitle(&NSString::from_str(title));
    }
}

/// The Accessibility and Microphone checklist, shared by the assistant and
/// the Permissions tab in Settings. Button tags: 0 Accessibility, 1 Microphone.
pub fn permission_card(
    mtm: MainThreadMarker,
    target: &AnyObject,
) -> (Retained<NSBox>, [PermissionRow; 2]) {
    let make = |tag: isize, title: &str, detail: &str| {
        let row = PermissionRow {
            status: symbol(mtm, "xmark.circle.fill", 18.0, &NSColor::systemRedColor()),
            allowed: label(mtm, "Allowed"),
            button: button(mtm, "Allow…", target, sel!(allowPermission:), tag),
        };
        row.allowed.setTextColor(Some(&NSColor::systemGreenColor()));
        let trailing = stack(
            mtm,
            NSUserInterfaceLayoutOrientation::Horizontal,
            0.0,
            &[&row.allowed, &row.button],
        );
        let view = card_row(
            mtm,
            Some(&row.status),
            &semibold(mtm, title),
            &note(mtm, detail, CARD_INNER - 150.0),
            Some(&trailing),
            CARD_INNER,
        );
        (view, row)
    };
    let (acc_view, accessibility) = make(
        0,
        "Accessibility",
        "Notices your hotkey and types the text.",
    );
    let (mic_view, microphone) = make(1, "Microphone", "Hears you only while you hold the key.");
    let card = rows_card(mtm, &[&acc_view, &mic_view], PANE_WIDTH);
    (card, [accessibility, microphone])
}

/// Updates a checklist from what macOS reports right now; true if both are
/// allowed.
pub fn refresh_permissions(rows: &[PermissionRow; 2]) -> bool {
    let accessibility = permissions::accessibility();
    let microphone = permissions::microphone();
    rows[0].show(accessibility);
    rows[1].show(microphone);
    accessibility == Access::Allowed && microphone == Access::Allowed
}

pub struct Setup {
    pub window: Retained<NSWindow>,
    pages: Retained<NSTabView>,
    current: Cell<usize>,
    step: Retained<NSTextField>,
    back: Retained<NSButton>,
    next: Retained<NSButton>,
    permission_rows: [PermissionRow; 2],
    mic_menu: Retained<NSPopUpButton>,
    /// "Hold Right Option and speak" on the last page; follows the choice.
    how_to_line: Retained<NSTextField>,
}

impl Setup {
    /// Shows page `index` and updates the step label and buttons.
    pub fn go_to(&self, index: usize, controls: &Controls) {
        let index = index.min(PAGES.len() - 1);
        self.current.set(index);
        self.pages.selectTabViewItemAtIndex(index as isize);
        let step = format!("Step {} of {}", index + 1, PAGES.len());
        self.step.setStringValue(&NSString::from_str(&step));
        self.back.setHidden(index == 0);
        let title = if self.is_last() { "Finish" } else { "Continue" };
        self.next.setTitle(&NSString::from_str(title));
        if PAGES[index] == Page::Microphone {
            self.fill_mic_menu(controls);
        }
        self.refresh(controls);
    }

    /// Updates the checklist and enables Continue once the page is complete.
    pub fn refresh(&self, controls: &Controls) {
        let permitted = refresh_permissions(&self.permission_rows);
        let ready = match PAGES[self.current.get()] {
            Page::Model => controls.model().is_installed(),
            Page::Permissions => permitted,
            _ => true,
        };
        self.next.setEnabled(ready);
    }

    pub fn current(&self) -> usize {
        self.current.get()
    }

    pub fn is_last(&self) -> bool {
        self.current.get() == PAGES.len() - 1
    }

    /// Updates the how-to line after the hotkey or mode changes.
    pub fn show_hotkey(&self, hotkey: &str, mode: Mode) {
        self.how_to_line
            .setStringValue(&NSString::from_str(&how_to(hotkey, mode)));
    }

    fn fill_mic_menu(&self, controls: &Controls) {
        let current = controls.input_device();
        let names = audio::input_device_names();
        self.mic_menu.removeAllItems();
        self.mic_menu
            .addItemWithTitle(&NSString::from_str("System Default"));
        for name in &names {
            self.mic_menu.addItemWithTitle(&NSString::from_str(name));
        }
        let selected = current
            .and_then(|c| names.iter().position(|n| *n == c))
            .map_or(0, |i| i + 1);
        self.mic_menu.selectItemAtIndex(selected as isize);
    }
}

fn how_to(hotkey: &str, mode: Mode) -> String {
    let key = hotkey_name(hotkey);
    match mode {
        Mode::Hold => format!("Hold {key} and speak"),
        Mode::Toggle => format!("Press {key}, speak, press it again"),
    }
}

/// Builds the assistant window; the model cards are returned so they can be
/// refreshed with the ones in Settings.
pub fn build(mtm: MainThreadMarker, actions: &Actions, cfg: &Config) -> (Setup, Vec<ModelRow>) {
    let target: &AnyObject = actions;
    let (model_page, rows) = model_page(mtm, target);
    let (permissions_page, permission_rows) = permissions_page(mtm, target);
    let (mic_page, mic_menu) = microphone_page(mtm, target);
    let (done_page, how_to_line) = done_page(mtm, cfg);
    let page_views = [
        welcome_page(mtm),
        model_page,
        permissions_page,
        mic_page,
        hotkey_page(mtm, target, cfg),
        text_page(mtm, target, actions.controls()),
        done_page,
    ];

    let pages = NSTabView::new(mtm);
    pages.setTabViewType(NSTabViewType::NoTabsNoBorder);
    let mut height: f64 = 0.0;
    for view in &page_views {
        view.layoutSubtreeIfNeeded();
        height = height.max(view.fittingSize().height);
        let item = NSTabViewItem::new();
        item.setView(Some(view));
        pages.addTabViewItem(&item);
    }
    let width = PANE_WIDTH + 80.0;
    fixed_width(&pages, width);
    fixed_height(&pages, height);

    let step = label(mtm, "");
    step.setFont(Some(&NSFont::systemFontOfSize(11.0)));
    step.setTextColor(Some(&NSColor::secondaryLabelColor()));
    let back = button(mtm, "Back", target, sel!(setupBack:), 0);
    let next = button(mtm, "Continue", target, sel!(setupNext:), 0);
    for b in [&back, &next] {
        b.setControlSize(NSControlSize::Regular);
    }
    // Return presses Continue, as in other Mac assistants.
    next.setKeyEquivalent(&NSString::from_str("\r"));
    let horizontal = NSUserInterfaceLayoutOrientation::Horizontal;
    let bar = stack(mtm, horizontal, 8.0, &[&step, &spacer(mtm), &back, &next]);
    bar.setDistribution(NSStackViewDistribution::Fill);
    bar.setEdgeInsets(NSEdgeInsets {
        top: 12.0,
        left: 24.0,
        bottom: 16.0,
        right: 24.0,
    });
    fixed_width(&bar, width);

    let content = stack(
        mtm,
        NSUserInterfaceLayoutOrientation::Vertical,
        0.0,
        &[&pages, &separator(mtm, width), &bar],
    );

    let frame = NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(width, height + 60.0));
    // Closing only hides the window: "Continue Setup…" in the menu bar brings
    // it back on the same page, and it reappears next launch until finished.
    // Not resizable: the pages have a fixed layout.
    let style =
        NSWindowStyleMask::Titled | NSWindowStyleMask::Closable | NSWindowStyleMask::Miniaturizable;
    // SAFETY: standard NSWindow initializer with a valid frame and style.
    let window = unsafe {
        NSWindow::initWithContentRect_styleMask_backing_defer(
            NSWindow::alloc(mtm),
            frame,
            style,
            NSBackingStoreType::Buffered,
            false,
        )
    };
    // SAFETY: the window is owned by `Retained` handles, so AppKit must not
    // also release it when it closes.
    unsafe { window.setReleasedWhenClosed(false) };
    window.setTitle(&NSString::from_str("Set up sayit"));
    window.setContentView(Some(&content));
    window.setContentSize(content.fittingSize());
    window.center();

    let setup = Setup {
        window,
        pages,
        current: Cell::new(0),
        step,
        back,
        next,
        permission_rows,
        mic_menu,
        how_to_line,
    };
    (setup, rows)
}

/// Brings a window to the front (sayit has no Dock icon).
pub fn bring_to_front(mtm: MainThreadMarker, window: &NSWindow) {
    window.makeKeyAndOrderFront(None);
    // `activate` needs macOS 14; sayit supports 13.
    #[allow(deprecated)]
    NSApplication::sharedApplication(mtm).activateIgnoringOtherApps(true);
}

/// A page: `icon`, a title and a one-line subtitle, centred above `body`,
/// with the whole group centred vertically in the window.
fn page(
    mtm: MainThreadMarker,
    icon: &NSView,
    title: &str,
    subtitle: &str,
    body: &[&NSView],
) -> Retained<NSStackView> {
    let title_label = label(mtm, title);
    // SAFETY: reading an immutable AppKit constant that is set at load time.
    let weight = unsafe { objc2_app_kit::NSFontWeightSemibold };
    title_label.setFont(Some(&NSFont::systemFontOfSize_weight(24.0, weight)));
    centered(&title_label);
    let subtitle_label = note(mtm, subtitle, PANE_WIDTH - 60.0);
    subtitle_label.setFont(Some(&NSFont::systemFontOfSize(13.0)));
    centered(&subtitle_label);

    let page = NSStackView::new(mtm);
    page.setOrientation(NSUserInterfaceLayoutOrientation::Vertical);
    page.setAlignment(NSLayoutAttribute::CenterX);
    page.setSpacing(8.0);
    page.setEdgeInsets(NSEdgeInsets {
        top: 28.0,
        left: 40.0,
        bottom: 28.0,
        right: 40.0,
    });
    let header: [&NSView; 3] = [icon, &title_label, &subtitle_label];
    for view in header.into_iter().chain(body.iter().copied()) {
        page.addView_inGravity(view, NSStackViewGravity::Center);
    }
    page.setCustomSpacing_afterView(14.0, icon);
    page.setCustomSpacing_afterView(24.0, &subtitle_label);
    page
}

fn badge(mtm: MainThreadMarker, symbol_name: &str, color: &NSColor) -> Retained<NSView> {
    Retained::into_super(icon_badge(mtm, symbol_name, 64.0, color))
}

/// sayit's icon: from the app bundle, or from the repository when running a
/// development build (which macOS would show with a generic icon).
fn app_icon(mtm: MainThreadMarker) -> Retained<NSView> {
    let exe = std::env::current_exe().ok();
    let image = exe
        .as_deref()
        .and_then(|e| e.parent())
        .into_iter()
        .flat_map(|d| {
            [
                d.join("../Resources/AppIcon.icns"),
                d.join("../../assets/AppIcon.icns"),
            ]
        })
        .find(|p| p.is_file())
        .and_then(|p| {
            let path = NSString::from_str(&p.to_string_lossy());
            NSImage::initWithContentsOfFile(NSImage::alloc(), &path)
        });
    let view = match image {
        Some(image) => NSImageView::imageViewWithImage(&image, mtm),
        None => NSImageView::new(mtm),
    };
    fixed_width(&view, 96.0);
    fixed_height(&view, 96.0);
    Retained::into_super(Retained::into_super(view))
}

fn welcome_page(mtm: MainThreadMarker) -> Retained<NSStackView> {
    let features = [
        (
            "lock.fill",
            NSColor::systemBlueColor(),
            "Private",
            "Nothing you say leaves this Mac.",
        ),
        (
            "keyboard.fill",
            NSColor::systemPurpleColor(),
            "Works in any app",
            "Hold a key, speak, and your words appear.",
        ),
        (
            "bolt.fill",
            NSColor::systemOrangeColor(),
            "Light",
            "Runs on the CPU in about 1 GB of memory.",
        ),
    ];
    let rows: Vec<_> = features
        .into_iter()
        .map(|(icon, color, title, text)| {
            let icon = icon_badge(mtm, icon, 34.0, &color);
            card_row(
                mtm,
                Some(&icon),
                &semibold(mtm, title),
                &note(mtm, text, CARD_INNER - 60.0),
                None,
                CARD_INNER,
            )
        })
        .collect();
    let refs: Vec<&NSView> = rows.iter().map(|r| -> &NSView { r }).collect();
    let card = rows_card(mtm, &refs, PANE_WIDTH);
    let legal = note(
        mtm,
        "Free and open source · provided as is · see DISCLAIMER.md",
        PANE_WIDTH,
    );
    centered(&legal);
    page(
        mtm,
        &app_icon(mtm),
        "Welcome to sayit",
        "Private dictation that runs entirely on your Mac.",
        &[&card, &legal],
    )
}

fn model_page(mtm: MainThreadMarker, target: &AnyObject) -> (Retained<NSStackView>, Vec<ModelRow>) {
    let (cards, rows): (Vec<_>, Vec<_>) = ModelId::ALL
        .into_iter()
        .enumerate()
        .map(|(i, model)| model_card(mtm, target, i, model))
        .unzip();
    let refs: Vec<&NSView> = cards.iter().map(|c| -> &NSView { c }).collect();
    let body = stack(mtm, NSUserInterfaceLayoutOrientation::Vertical, 10.0, &refs);
    let page = page(
        mtm,
        &badge(mtm, "cpu", &NSColor::systemBlueColor()),
        "Choose a speech model",
        "Parakeet v2 suits most Macs. You can switch anytime in Settings.",
        &[&body],
    );
    (page, rows)
}

fn permissions_page(
    mtm: MainThreadMarker,
    target: &AnyObject,
) -> (Retained<NSStackView>, [PermissionRow; 2]) {
    let (card, rows) = permission_card(mtm, target);
    let footer = note(
        mtm,
        "Both belong to sayit alone; you can revoke them anytime in System Settings. \
         Trouble? Settings › Permissions has a step-by-step guide.",
        PANE_WIDTH,
    );
    centered(&footer);
    let page = page(
        mtm,
        &badge(mtm, "lock.shield.fill", &NSColor::systemGreenColor()),
        "Allow access",
        "sayit needs two permissions. Nothing you say leaves your Mac.",
        &[&card, &footer],
    );
    (page, rows)
}

fn microphone_page(
    mtm: MainThreadMarker,
    target: &AnyObject,
) -> (Retained<NSStackView>, Retained<NSPopUpButton>) {
    let menu = popup(mtm, &["System Default"], 0, target, sel!(setupMicrophone:));
    let row = card_row(
        mtm,
        None,
        &semibold(mtm, "Input"),
        &note(mtm, "Used whenever you dictate.", CARD_INNER - 220.0),
        Some(&menu),
        CARD_INNER,
    );
    let card = rows_card(mtm, &[&row], PANE_WIDTH);
    let tip = note(
        mtm,
        "Tip: the built-in mic starts instantly. Bluetooth headsets take about a second, \
         which can cut off your first words.",
        PANE_WIDTH - 40.0,
    );
    centered(&tip);
    let page = page(
        mtm,
        &badge(mtm, "mic.fill", &NSColor::systemOrangeColor()),
        "Choose a microphone",
        "Pick the mic sayit listens to. You can change it from the menu bar.",
        &[&card, &tip],
    );
    (page, menu)
}

fn hotkey_page(mtm: MainThreadMarker, target: &AnyObject, cfg: &Config) -> Retained<NSStackView> {
    let mut titles: Vec<String> = HOTKEYS.iter().map(|k| hotkey_name(k)).collect();
    let selected = match HOTKEYS.iter().position(|k| *k == cfg.hotkey) {
        Some(i) => i,
        None => {
            // A key set by hand in config.toml: keep it as the last choice.
            titles.push(hotkey_name(&cfg.hotkey));
            titles.len() - 1
        }
    };
    let title_refs: Vec<&str> = titles.iter().map(String::as_str).collect();
    let key_menu = popup(mtm, &title_refs, selected, target, sel!(setupHotkey:));
    let key_row = card_row(
        mtm,
        None,
        &semibold(mtm, "Hotkey"),
        &note(mtm, "A key you don't use for typing.", CARD_INNER - 220.0),
        Some(&key_menu),
        CARD_INNER,
    );
    let mode = segmented(
        mtm,
        &["Hold", "Toggle"],
        (cfg.mode == Mode::Toggle) as usize,
        target,
        sel!(setupMode:),
    );
    let mode_row = card_row(
        mtm,
        None,
        &semibold(mtm, "How it works"),
        &note(
            mtm,
            "Hold: speak while held. Toggle: press to start and stop.",
            CARD_INNER - 180.0,
        ),
        Some(&mode),
        CARD_INNER,
    );
    let card = rows_card(mtm, &[&key_row, &mode_row], PANE_WIDTH);
    page(
        mtm,
        &badge(mtm, "keyboard.fill", &NSColor::systemPurpleColor()),
        "Pick your hotkey",
        "Right Option works well: it's rarely used and easy to reach.",
        &[&card],
    )
}

fn text_page(
    mtm: MainThreadMarker,
    target: &AnyObject,
    controls: &Controls,
) -> Retained<NSStackView> {
    let rows: Vec<_> = Toggle::ALL
        .into_iter()
        .enumerate()
        .map(|(i, toggle)| {
            let on = toggle.flag(controls).load(Ordering::Relaxed);
            let control = switch(mtm, on, target, sel!(toggleSetting:), i as isize);
            card_row(
                mtm,
                None,
                &label(mtm, toggle.title()),
                &note(mtm, toggle.explanation(), CARD_INNER - 80.0),
                Some(&control),
                CARD_INNER,
            )
        })
        .collect();
    let refs: Vec<&NSView> = rows.iter().map(|r| -> &NSView { r }).collect();
    let card = rows_card(mtm, &refs, PANE_WIDTH);
    page(
        mtm,
        &badge(mtm, "textformat", &NSColor::systemTealColor()),
        "Tidy up your text",
        "How sayit types what you say. You can change these in Settings.",
        &[&card],
    )
}

fn done_page(
    mtm: MainThreadMarker,
    cfg: &Config,
) -> (Retained<NSStackView>, Retained<NSTextField>) {
    let how_to_line = semibold(mtm, &how_to(&cfg.hotkey, cfg.mode));
    let rows = [
        (
            "1.circle.fill",
            semibold(mtm, "Click where you want to type"),
            "Any app, any text field.",
        ),
        (
            "2.circle.fill",
            how_to_line.clone(),
            "The menu-bar icon fills in while it listens.",
        ),
        (
            "3.circle.fill",
            semibold(mtm, "Release, and your words appear"),
            "Typed right at your cursor.",
        ),
    ]
    .map(|(icon, title, text)| {
        let icon = symbol(mtm, icon, 22.0, &NSColor::controlAccentColor());
        card_row(
            mtm,
            Some(&icon),
            &title,
            &note(mtm, text, CARD_INNER - 60.0),
            None,
            CARD_INNER,
        )
    });
    let refs: Vec<&NSView> = rows.iter().map(|r| -> &NSView { r }).collect();
    let card = rows_card(mtm, &refs, PANE_WIDTH);
    let footer = note(
        mtm,
        "sayit now lives in the menu bar: click its icon to pause or open Settings.",
        PANE_WIDTH,
    );
    centered(&footer);
    let page = page(
        mtm,
        &badge(mtm, "checkmark", &NSColor::systemGreenColor()),
        "You're all set",
        "Dictate anywhere in three steps.",
        &[&card, &footer],
    );
    (page, how_to_line)
}
