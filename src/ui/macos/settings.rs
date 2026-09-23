//! The Settings window. Changes apply immediately and are saved to config.toml.

use std::sync::atomic::{AtomicBool, Ordering};

use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::{MainThreadMarker, sel};
use objc2_app_kit::{
    NSBox, NSButton, NSColor, NSFont, NSImage, NSImageView, NSLayoutAttribute, NSProgressIndicator,
    NSStackView, NSStackViewDistribution, NSSwitch, NSTabViewController,
    NSTabViewControllerTabStyle, NSTabViewItem, NSTextAlignment, NSTextField,
    NSUserInterfaceLayoutOrientation, NSView, NSViewController, NSWindow, NSWindowStyleMask,
};
use objc2_foundation::{NSEdgeInsets, NSSize, NSString};

use super::actions::Actions;
use super::login::Login;
use super::setup::{HotkeyPicker, PermissionRow, app_icon_image, hotkey_card, permission_card};
use super::widgets::{
    button, card, card_row, check_state, fixed_height, fixed_width, label, note, popup,
    progress_bar, rows_card, semibold, spacer, stack, switch,
};
use crate::config::{Config, OnClose};
use crate::daemon::Controls;
use crate::stt::ModelId;

/// On/off settings shown as switches. The switch tag is the index in `ALL`
/// (declaration order).
#[derive(Clone, Copy)]
pub enum Toggle {
    RemoveFillers,
    NewlineAfterTake,
    KeepHistory,
}

impl Toggle {
    pub const ALL: [Toggle; 3] = [
        Toggle::RemoveFillers,
        Toggle::NewlineAfterTake,
        Toggle::KeepHistory,
    ];

    pub fn title(self) -> &'static str {
        match self {
            Toggle::RemoveFillers => "Remove fillers",
            Toggle::NewlineAfterTake => "New line",
            Toggle::KeepHistory => "Clipboard history",
        }
    }

    pub fn explanation(self) -> &'static str {
        match self {
            Toggle::RemoveFillers => "Drops \"um\", \"uh\" and \"erm\" from what you say.",
            Toggle::NewlineAfterTake => {
                "Starts each dictation on a new line. Off adds a space instead; turn it off for terminals."
            }
            Toggle::KeepHistory => {
                "Your last few dictations in the menu bar's Clipboard menu, to copy again. \
                 Kept in memory only, gone when sayit quits."
            }
        }
    }

    /// The live switch the dictation service reads.
    pub fn flag(self, controls: &Controls) -> &AtomicBool {
        match self {
            Toggle::RemoveFillers => &controls.remove_fillers,
            Toggle::NewlineAfterTake => &controls.newline_after_take,
            Toggle::KeepHistory => &controls.keep_history,
        }
    }

    /// The same setting in config.toml.
    pub fn field(self, cfg: &mut Config) -> &mut bool {
        match self {
            Toggle::RemoveFillers => &mut cfg.remove_fillers,
            Toggle::NewlineAfterTake => &mut cfg.newline_after_take,
            Toggle::KeepHistory => &mut cfg.keep_history,
        }
    }
}

/// The choices for closing the Settings window, in popup order.
pub const ON_CLOSE: [(OnClose, &str); 3] = [
    (OnClose::MenuBar, "Keep running in the menu bar"),
    (OnClose::Dock, "Keep running, also in the Dock"),
    (OnClose::Quit, "Quit sayit"),
];

/// The About tab's links: name, description, address. Buttons carry the
/// index as their tag, so only these fixed addresses can ever be opened.
pub const LINKS: [(&str, &str, &str); 5] = [
    (
        "Help",
        "Fixes for common problems.",
        "https://github.com/ipsamurai/sayit#troubleshooting",
    ),
    (
        "Report bug",
        "Opens a new issue on GitHub.",
        "https://github.com/ipsamurai/sayit/issues/new/choose",
    ),
    (
        "Source code",
        "sayit is open source on GitHub.",
        "https://github.com/ipsamurai/sayit",
    ),
    (
        "Privacy",
        "What sayit keeps, and what it never does.",
        "https://github.com/ipsamurai/sayit/blob/main/PRIVACY.md",
    ),
    (
        "Disclaimer",
        "sayit is provided as is, without warranty.",
        "https://github.com/ipsamurai/sayit/blob/main/DISCLAIMER.md",
    ),
];

/// Choices for how many recent dictations to keep.
pub const HISTORY_SIZES: [usize; 3] = [3, 5, 10];

/// Width of every tab's content; cards and text are laid out to fit it.
pub const PANE_WIDTH: f64 = 520.0;
/// Width inside a card (the card adds 14 pt margins on each side).
const CARD_INNER: f64 = PANE_WIDTH - 28.0;

/// The controls of one model card. Button tags are the model's index in
/// `ModelId::ALL`.
pub struct ModelRow {
    model: ModelId,
    badge: Retained<NSTextField>,
    status: Retained<NSTextField>,
    progress: Retained<NSProgressIndicator>,
    download: Retained<NSButton>,
    choose: Retained<NSButton>,
    delete: Retained<NSButton>,
}

/// What the Models tab shows, besides what's on disk.
pub struct ModelsState<'a> {
    pub current: ModelId,
    /// The model being downloaded and how far along it is (0 to 1).
    pub downloading: Option<(ModelId, f64)>,
    /// The last failed download and why.
    pub error: Option<&'a (ModelId, String)>,
}

/// Updates every model card to match what's installed and what's happening.
pub fn refresh_models(rows: &[ModelRow], state: &ModelsState) {
    for row in rows {
        let model = row.model;
        let installed = model.is_installed();
        let current = model == state.current;
        let progress = state
            .downloading
            .and_then(|(m, p)| (m == model).then_some(p));
        let busy_elsewhere = state.downloading.is_some() && progress.is_none();
        let error = state.error.filter(|(m, _)| *m == model).map(|(_, e)| e);
        let size = format!("{} MB", model.download_bytes() / 1_000_000);

        let (badge, color) = if current && installed {
            ("In use", NSColor::systemGreenColor())
        } else if model == ModelId::DEFAULT {
            ("Recommended", NSColor::systemBlueColor())
        } else {
            ("", NSColor::secondaryLabelColor())
        };
        row.badge.setStringValue(&NSString::from_str(badge));
        row.badge.setTextColor(Some(&color));

        let status = match (progress, error) {
            (Some(p), _) => format!("Downloading… {:.0}% of {size}", p * 100.0),
            (None, Some(e)) => format!("Download failed: {e}"),
            (None, None) if installed => format!("Downloaded · {size}"),
            (None, None) => format!("Not downloaded · {size}"),
        };
        row.status.setStringValue(&NSString::from_str(&status));
        row.progress.setHidden(progress.is_none());
        row.progress.setDoubleValue(progress.unwrap_or(0.0));

        let title = if progress.is_some() {
            "Cancel"
        } else {
            "Download"
        };
        row.download.setTitle(&NSString::from_str(title));
        row.download.setHidden(installed && progress.is_none());
        row.download.setEnabled(!busy_elsewhere);
        row.choose.setHidden(!installed || current);
        // The model in use can't be deleted; choose another one first.
        row.delete.setHidden(!installed || current);
    }
}

/// Builds the window: a toolbar of tabs, like the settings of other Mac apps.
/// It's created once and hidden rather than released when closed, so
/// reopening it is instant.
pub fn build(mtm: MainThreadMarker, actions: &Actions) -> (SettingsWindow, Vec<ModelRow>) {
    let target: &AnyObject = actions;
    let cfg = Config::load().unwrap_or_default();
    let (general_pane, hotkey, login_switch, login_note) = general_pane(mtm, target, &cfg);
    let (models_pane, rows) = models_pane(mtm, target);
    let controls = actions.controls();
    let text_pane = text_pane(mtm, target, controls);
    let clipboard_pane = clipboard_pane(mtm, target, controls, &cfg);
    let (permissions_pane, permission_rows, mic_test) = permissions_pane(mtm, target);
    let about_pane = about_pane(mtm, target);

    let tabs = NSTabViewController::new(mtm);
    tabs.setTabStyle(NSTabViewControllerTabStyle::Toolbar);
    for (label, symbol, pane) in [
        ("General", "gearshape", &general_pane),
        ("Models", "cpu", &models_pane),
        ("Text", "textformat", &text_pane),
        ("Clipboard", "doc.on.clipboard", &clipboard_pane),
        ("Permissions", "lock.shield", &permissions_pane),
        ("About", "info.circle", &about_pane),
    ] {
        let controller = NSViewController::new(mtm);
        controller.setView(pane);
        controller.setTitle(Some(&NSString::from_str(label)));
        pane.layoutSubtreeIfNeeded();
        // Same width for every tab, so the window doesn't jump when switching.
        let height = pane.fittingSize().height;
        controller.setPreferredContentSize(NSSize::new(PANE_WIDTH + 48.0, height));
        let item = NSTabViewItem::tabViewItemWithViewController(&controller);
        item.setLabel(&NSString::from_str(label));
        item.setImage(
            NSImage::imageWithSystemSymbolName_accessibilityDescription(
                &NSString::from_str(symbol),
                None,
            )
            .as_deref(),
        );
        tabs.addTabViewItem(&item);
    }

    let window = NSWindow::windowWithContentViewController(&tabs);
    window.setStyleMask(NSWindowStyleMask::Titled | NSWindowStyleMask::Closable);
    // SAFETY: the window is owned by `Retained` handles, so AppKit must not
    // also release it when it closes.
    unsafe { window.setReleasedWhenClosed(false) };
    window.center();
    let settings = SettingsWindow {
        window,
        tabs,
        hotkey,
        login_switch,
        login_note,
        permission_rows,
        mic_test,
    };
    (settings, rows)
}

/// The Settings window and the parts of it that change while it's open.
pub struct SettingsWindow {
    pub window: Retained<NSWindow>,
    tabs: Retained<NSTabViewController>,
    pub hotkey: HotkeyPicker,
    login_switch: Retained<NSSwitch>,
    login_note: Retained<NSTextField>,
    pub permission_rows: [PermissionRow; 2],
    /// Result of the last microphone test.
    pub mic_test: Retained<NSTextField>,
}

impl SettingsWindow {
    pub fn select_tab(&self, tab: SettingsTab) {
        self.tabs.setSelectedTabViewItemIndex(tab as isize);
    }

    /// Shows the login item as macOS reports it (the user can also change it
    /// in System Settings).
    pub fn show_login(&self, login: Login) {
        let text = match login {
            Login::Unavailable => "Works in the installed sayit.app, not when run from a terminal.",
            Login::NeedsApproval => "Allow sayit in System Settings › General › Login Items.",
            Login::On | Login::Off => "Opens sayit in the menu bar when you log in to your Mac.",
        };
        self.login_note.setStringValue(&NSString::from_str(text));
        self.login_switch.setState(check_state(matches!(
            login,
            Login::On | Login::NeedsApproval
        )));
        self.login_switch.setEnabled(login != Login::Unavailable);
    }
}

/// Tabs in the order they're added in `build`.
#[derive(Clone, Copy)]
pub enum SettingsTab {
    Models = 1,
    Permissions = 4,
}

/// Name, version and credits, with links to help and the project's docs.
fn about_pane(mtm: MainThreadMarker, target: &AnyObject) -> Retained<NSStackView> {
    let icon = match app_icon_image() {
        Some(image) => NSImageView::imageViewWithImage(&image, mtm),
        None => NSImageView::new(mtm),
    };
    fixed_width(&icon, 64.0);
    fixed_height(&icon, 64.0);
    let name = label(mtm, "sayit");
    name.setFont(Some(&NSFont::boldSystemFontOfSize(20.0)));
    let version = note(
        mtm,
        &format!(
            "Version {}\nPrivate, local dictation for lower-end devices.",
            env!("CARGO_PKG_VERSION")
        ),
        PANE_WIDTH,
    );
    version.setAlignment(NSTextAlignment::Center);
    let header = stack(
        mtm,
        NSUserInterfaceLayoutOrientation::Vertical,
        6.0,
        &[&icon, &name, &version],
    );

    let rows: Vec<_> = LINKS
        .iter()
        .enumerate()
        .map(|(i, (name, detail, _))| {
            let open = button(mtm, "Open", target, sel!(openLink:), i as isize);
            card_row(
                mtm,
                None,
                &label(mtm, name),
                &note(mtm, detail, CARD_INNER - 100.0),
                Some(&open),
                CARD_INNER,
            )
        })
        .collect();
    let refs: Vec<&NSView> = rows.iter().map(|r| -> &NSView { r }).collect();
    let credits = note(
        mtm,
        "© 2026 ipsamurai and the sayit contributors. Licensed under MIT OR Apache-2.0.\n\
         Speech models: NVIDIA Parakeet (CC-BY-4.0) and Moonshine AI (MIT).",
        PANE_WIDTH,
    );
    let pane = pane(
        mtm,
        &[&header, &rows_card(mtm, &refs, PANE_WIDTH), &credits],
    );
    pane.setAlignment(NSLayoutAttribute::CenterX);
    pane
}

/// The hotkey, start at login, and what closing this window does.
fn general_pane(
    mtm: MainThreadMarker,
    target: &AnyObject,
    cfg: &Config,
) -> (
    Retained<NSStackView>,
    HotkeyPicker,
    Retained<NSSwitch>,
    Retained<NSTextField>,
) {
    let (hotkey_card, hotkey) = hotkey_card(mtm, target, cfg);
    let login_switch = switch(mtm, false, target, sel!(toggleLogin:), 0);
    let login_note = note(mtm, "", CARD_INNER - 70.0);
    let login = card_row(
        mtm,
        None,
        &label(mtm, "Autostart"),
        &login_note,
        Some(&login_switch),
        CARD_INNER,
    );
    let titles = ON_CLOSE.map(|(_, title)| title);
    let selected = ON_CLOSE.iter().position(|(c, _)| *c == cfg.on_close);
    let choice = popup(
        mtm,
        &titles,
        selected.unwrap_or(0),
        target,
        sel!(chooseOnClose:),
    );
    let closing = card_row(
        mtm,
        None,
        &label(mtm, "On close"),
        &note(
            mtm,
            "What closing this window does. Unless you choose Quit, your hotkey keeps working.",
            CARD_INNER - 250.0,
        ),
        Some(&choice),
        CARD_INNER,
    );
    let card = rows_card(mtm, &[&login, &closing], PANE_WIDTH);
    let pane = pane(mtm, &[&hotkey_card, &card]);
    (pane, hotkey, login_switch, login_note)
}

/// The fallback when setup was skipped or a permission was later revoked:
/// the live checklist, a microphone test that proves sayit really hears you,
/// and the steps to fix things by hand.
fn permissions_pane(
    mtm: MainThreadMarker,
    target: &AnyObject,
) -> (
    Retained<NSStackView>,
    [PermissionRow; 2],
    Retained<NSTextField>,
) {
    let intro = note(
        mtm,
        "What macOS reports right now. A green tick means sayit really has the permission.",
        PANE_WIDTH,
    );
    let (checklist, rows) = permission_card(mtm, target);

    let mic_test = label(
        mtm,
        "Records 1.5 seconds, then shows whether sayit heard you.",
    );
    mic_test.setFont(Some(&NSFont::systemFontOfSize(11.0)));
    mic_test.setTextColor(Some(&NSColor::secondaryLabelColor()));
    let test = button(mtm, "Test Microphone", target, sel!(testMicrophone:), 0);
    let check = button(mtm, "Check Again", target, sel!(checkPermissions:), 0);
    let horizontal = NSUserInterfaceLayoutOrientation::Horizontal;
    let tools = stack(mtm, horizontal, 8.0, &[&test, &check, &mic_test]);

    let guide = note(
        mtm,
        "If something stays red:\n\
         1. Open System Settings › Privacy & Security › Accessibility and turn on sayit. \
         If it isn't listed, click +, choose sayit in Applications, and turn it on.\n\
         2. If sayit is on but still red (common after an update), select it, click −, \
         then add it again.\n\
         3. In Privacy & Security › Microphone, turn on sayit.\n\
         Running sayit from a terminal? Allow the terminal app instead, then quit and \
         reopen it.",
        PANE_WIDTH,
    );
    let pane = pane(mtm, &[&intro, &checklist, &tools, &guide]);
    (pane, rows, mic_test)
}

/// A tab's content: a column of views with standard window margins.
pub fn pane(mtm: MainThreadMarker, views: &[&NSView]) -> Retained<NSStackView> {
    let pane = stack(mtm, NSUserInterfaceLayoutOrientation::Vertical, 12.0, views);
    pane.setEdgeInsets(NSEdgeInsets {
        top: 20.0,
        left: 24.0,
        bottom: 24.0,
        right: 24.0,
    });
    fixed_width(&pane, PANE_WIDTH + 48.0);
    pane
}

fn models_pane(
    mtm: MainThreadMarker,
    target: &AnyObject,
) -> (Retained<NSStackView>, Vec<ModelRow>) {
    let intro = note(
        mtm,
        "Models run entirely on this Mac. Larger ones are more accurate but use more memory.",
        PANE_WIDTH,
    );
    let (cards, rows): (Vec<_>, Vec<_>) = ModelId::ALL
        .into_iter()
        .enumerate()
        .map(|(i, model)| model_card(mtm, target, i, model))
        .unzip();
    let footer = note(
        mtm,
        "Every download is checked against a pinned SHA-256 checksum.",
        PANE_WIDTH,
    );
    let mut refs: Vec<&NSView> = vec![&intro];
    refs.extend(cards.iter().map(|card| -> &NSView { card }));
    refs.push(&footer);
    (pane(mtm, &refs), rows)
}

pub fn model_card(
    mtm: MainThreadMarker,
    target: &AnyObject,
    i: usize,
    model: ModelId,
) -> (Retained<NSBox>, ModelRow) {
    let tag = i as isize;
    let row = ModelRow {
        model,
        badge: label(mtm, ""),
        status: label(mtm, ""),
        progress: progress_bar(mtm),
        download: button(mtm, "Download", target, sel!(downloadModel:), tag),
        choose: button(mtm, "Use", target, sel!(useModel:), tag),
        delete: button(mtm, "Delete", target, sel!(deleteModel:), tag),
    };
    row.badge.setFont(Some(&NSFont::boldSystemFontOfSize(11.0)));
    row.status.setFont(Some(&NSFont::systemFontOfSize(11.0)));
    row.status
        .setTextColor(Some(&NSColor::secondaryLabelColor()));
    fixed_width(&row.progress, 140.0);

    let horizontal = NSUserInterfaceLayoutOrientation::Horizontal;
    let name = semibold(mtm, model.label());
    let buttons = stack(
        mtm,
        horizontal,
        6.0,
        &[&row.download, &row.choose, &row.delete],
    );
    // Top line: name and badge, a spacer that takes the free space, then the
    // buttons, so they stay at the right edge whichever are visible. Fixed
    // heights keep every card the same size as buttons come and go.
    let heading = stack(mtm, horizontal, 8.0, &[&name, &row.badge]);
    let top = stack(mtm, horizontal, 8.0, &[&heading, &spacer(mtm), &buttons]);
    top.setDistribution(NSStackViewDistribution::Fill);
    fixed_width(&top, CARD_INNER);
    fixed_height(&top, 22.0);

    let status_line = stack(mtm, horizontal, 8.0, &[&row.progress, &row.status]);
    fixed_height(&status_line, 16.0);
    let content = stack(
        mtm,
        NSUserInterfaceLayoutOrientation::Vertical,
        6.0,
        &[&top, &specs_line(mtm, model), &status_line],
    );
    (card(mtm, &content, PANE_WIDTH), row)
}

/// "Languages English | RAM 1.2 GB | Speed … | Accuracy …", with the field
/// names dimmed so the values stand out.
fn specs_line(mtm: MainThreadMarker, model: ModelId) -> Retained<NSStackView> {
    let line = NSStackView::new(mtm);
    line.setOrientation(NSUserInterfaceLayoutOrientation::Horizontal);
    line.setSpacing(5.0);
    for (i, (name, value)) in model.specs().into_iter().enumerate() {
        if i > 0 {
            line.addArrangedSubview(&small_label(mtm, "|", NSColor::tertiaryLabelColor()));
        }
        line.addArrangedSubview(&small_label(mtm, name, NSColor::secondaryLabelColor()));
        line.addArrangedSubview(&small_label(mtm, value, NSColor::labelColor()));
    }
    line
}

fn small_label(
    mtm: MainThreadMarker,
    text: &str,
    color: Retained<NSColor>,
) -> Retained<NSTextField> {
    let field = label(mtm, text);
    field.setFont(Some(&NSFont::systemFontOfSize(11.0)));
    field.setTextColor(Some(&color));
    field
}

/// Switch rows for some of the `Toggle`s.
fn toggle_rows(
    mtm: MainThreadMarker,
    target: &AnyObject,
    controls: &Controls,
    toggles: &[Toggle],
) -> Vec<Retained<NSStackView>> {
    toggles
        .iter()
        .map(|&toggle| {
            let on = toggle.flag(controls).load(Ordering::Relaxed);
            let control = switch(mtm, on, target, sel!(toggleSetting:), toggle as isize);
            card_row(
                mtm,
                None,
                &label(mtm, toggle.title()),
                &note(mtm, toggle.explanation(), CARD_INNER - 70.0),
                Some(&control),
                CARD_INNER,
            )
        })
        .collect()
}

fn text_pane(
    mtm: MainThreadMarker,
    target: &AnyObject,
    controls: &Controls,
) -> Retained<NSStackView> {
    let toggles = [Toggle::RemoveFillers, Toggle::NewlineAfterTake];
    let rows = toggle_rows(mtm, target, controls, &toggles);
    let refs: Vec<&NSView> = rows.iter().map(|r| -> &NSView { r }).collect();
    let intro = note(mtm, "Changes apply to your next dictation.", PANE_WIDTH);
    pane(mtm, &[&intro, &rows_card(mtm, &refs, PANE_WIDTH)])
}

/// The recent dictations in the menu bar's Clipboard menu.
fn clipboard_pane(
    mtm: MainThreadMarker,
    target: &AnyObject,
    controls: &Controls,
    cfg: &Config,
) -> Retained<NSStackView> {
    let toggles = [Toggle::KeepHistory];
    let mut rows = toggle_rows(mtm, target, controls, &toggles);
    let titles = HISTORY_SIZES.map(|n| n.to_string());
    let titles: Vec<&str> = titles.iter().map(String::as_str).collect();
    let selected = HISTORY_SIZES
        .iter()
        .position(|&n| n == cfg.history_size)
        .unwrap_or(HISTORY_SIZES.len() - 1);
    let size = popup(mtm, &titles, selected, target, sel!(chooseHistorySize:));
    rows.push(card_row(
        mtm,
        None,
        &label(mtm, "Keep last"),
        &note(
            mtm,
            "How many dictations to keep. The oldest drops off first.",
            CARD_INNER - 120.0,
        ),
        Some(&size),
        CARD_INNER,
    ));
    let refs: Vec<&NSView> = rows.iter().map(|r| -> &NSView { r }).collect();
    let intro = note(
        mtm,
        "Menu bar › Clipboard: click a dictation to copy it.",
        PANE_WIDTH,
    );
    pane(mtm, &[&intro, &rows_card(mtm, &refs, PANE_WIDTH)])
}

#[cfg(test)]
mod tests {
    use super::LINKS;

    #[test]
    fn about_links_stay_on_the_project() {
        for (_, _, url) in LINKS {
            assert!(
                url.starts_with("https://github.com/ipsamurai/sayit"),
                "{url}"
            );
        }
    }
}
