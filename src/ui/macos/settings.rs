//! The Settings window. Changes apply immediately and are saved to config.toml.

use std::sync::atomic::{AtomicBool, Ordering};

use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::{MainThreadMarker, sel};
use objc2_app_kit::{
    NSApplication, NSBox, NSButton, NSColor, NSFont, NSImage, NSLayoutAttribute,
    NSProgressIndicator, NSStackView, NSStackViewDistribution, NSStackViewGravity,
    NSTabViewController, NSTabViewControllerTabStyle, NSTabViewItem, NSTextField,
    NSUserInterfaceLayoutOrientation, NSView, NSViewController, NSWindow, NSWindowStyleMask,
};
use objc2_foundation::{NSEdgeInsets, NSSize, NSString};

use super::actions::Actions;
use super::widgets::{
    button, card, fixed_height, fixed_width, label, note, progress_bar, semibold, separator,
    spacer, stack, switch,
};
use crate::config::Config;
use crate::daemon::Controls;
use crate::stt::ModelId;

/// On/off settings shown as switches. The switch tag is the index in `ALL`.
#[derive(Clone, Copy)]
pub enum Toggle {
    RemoveFillers,
    NewlineAfterTake,
    RestoreClipboard,
}

impl Toggle {
    pub const ALL: [Toggle; 3] = [
        Toggle::RemoveFillers,
        Toggle::NewlineAfterTake,
        Toggle::RestoreClipboard,
    ];

    fn title(self) -> &'static str {
        match self {
            Toggle::RemoveFillers => "Remove filler words",
            Toggle::NewlineAfterTake => "Start each dictation on a new line",
            Toggle::RestoreClipboard => "Restore my clipboard after pasting",
        }
    }

    fn explanation(self) -> &'static str {
        match self {
            Toggle::RemoveFillers => "Drops \"um\", \"uh\" and \"erm\" from what you dictate.",
            Toggle::NewlineAfterTake => {
                "Otherwise a space is added. Turn this off if you dictate into terminals, \
                 where a new line can run a command."
            }
            Toggle::RestoreClipboard => {
                "sayit pastes through the clipboard. On: what you had copied before \
                 dictating is put back. Off: the dictated text stays on the clipboard."
            }
        }
    }

    /// The live switch the dictation service reads.
    pub fn flag(self, controls: &Controls) -> &AtomicBool {
        match self {
            Toggle::RemoveFillers => &controls.remove_fillers,
            Toggle::NewlineAfterTake => &controls.newline_after_take,
            Toggle::RestoreClipboard => &controls.restore_clipboard,
        }
    }

    /// The same setting in config.toml.
    pub fn field(self, cfg: &mut Config) -> &mut bool {
        match self {
            Toggle::RemoveFillers => &mut cfg.remove_fillers,
            Toggle::NewlineAfterTake => &mut cfg.newline_after_take,
            Toggle::RestoreClipboard => &mut cfg.restore_clipboard,
        }
    }
}

/// Width of every tab's content; cards and text are laid out to fit it.
const PANE_WIDTH: f64 = 520.0;
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
pub fn build(mtm: MainThreadMarker, actions: &Actions) -> (Retained<NSWindow>, Vec<ModelRow>) {
    let target: &AnyObject = actions;
    let (models_pane, rows) = models_pane(mtm, target);
    let text_pane = text_pane(mtm, target, actions.controls());

    let tabs = NSTabViewController::new(mtm);
    tabs.setTabStyle(NSTabViewControllerTabStyle::Toolbar);
    for (label, symbol, pane) in [
        ("Models", "cpu", &models_pane),
        ("Text", "textformat", &text_pane),
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
    (window, rows)
}

/// A tab's content: a column of views with standard window margins.
fn pane(mtm: MainThreadMarker, views: &[&NSView]) -> Retained<NSStackView> {
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
        "Choose the speech model sayit uses. Models run entirely on this Mac; \
         larger ones are more accurate but use more memory.",
        PANE_WIDTH,
    );
    let (cards, rows): (Vec<_>, Vec<_>) = ModelId::ALL
        .into_iter()
        .enumerate()
        .map(|(i, model)| model_card(mtm, target, i, model))
        .unzip();
    let footer = note(
        mtm,
        "Downloads come from Hugging Face through sayit's bundled script, and every file \
         is checked against a pinned SHA-256 checksum. After that, sayit works offline.",
        PANE_WIDTH,
    );
    let mut refs: Vec<&NSView> = vec![&intro];
    refs.extend(cards.iter().map(|card| -> &NSView { card }));
    refs.push(&footer);
    (pane(mtm, &refs), rows)
}

fn model_card(
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

fn text_pane(
    mtm: MainThreadMarker,
    target: &AnyObject,
    controls: &Controls,
) -> Retained<NSStackView> {
    let rows = NSStackView::new(mtm);
    rows.setOrientation(NSUserInterfaceLayoutOrientation::Vertical);
    rows.setSpacing(10.0);
    for (i, toggle) in Toggle::ALL.into_iter().enumerate() {
        if i > 0 {
            rows.addArrangedSubview(&separator(mtm, CARD_INNER));
        }
        let on = toggle.flag(controls).load(Ordering::Relaxed);
        let texts = stack(
            mtm,
            NSUserInterfaceLayoutOrientation::Vertical,
            3.0,
            &[
                &label(mtm, toggle.title()),
                &note(mtm, toggle.explanation(), CARD_INNER - 70.0),
            ],
        );
        let row = NSStackView::new(mtm);
        row.setOrientation(NSUserInterfaceLayoutOrientation::Horizontal);
        row.setAlignment(NSLayoutAttribute::CenterY);
        row.addView_inGravity(&texts, NSStackViewGravity::Leading);
        let switch = switch(mtm, on, target, sel!(toggleSetting:), i as isize);
        row.addView_inGravity(&switch, NSStackViewGravity::Trailing);
        fixed_width(&row, CARD_INNER);
        rows.addArrangedSubview(&row);
    }
    let intro = note(
        mtm,
        "How sayit tidies up and types what you say. Changes apply to your next dictation.",
        PANE_WIDTH,
    );
    pane(mtm, &[&intro, &card(mtm, &rows, PANE_WIDTH)])
}

/// Brings the window to the front. sayit has no Dock icon, so the app must
/// be activated explicitly or the window would open behind others.
pub fn show(mtm: MainThreadMarker, window: &NSWindow) {
    window.makeKeyAndOrderFront(None);
    // `activate` needs macOS 14; sayit supports 13.
    #[allow(deprecated)]
    NSApplication::sharedApplication(mtm).activateIgnoringOtherApps(true);
}
