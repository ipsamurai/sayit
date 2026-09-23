//! The Settings window. Changes apply immediately and are saved to config.toml.

use std::sync::atomic::{AtomicBool, Ordering};

use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::{MainThreadMarker, MainThreadOnly, sel};
use objc2_app_kit::{
    NSApplication, NSBackingStoreType, NSButton, NSLayoutAttribute, NSProgressIndicator,
    NSStackView, NSTextField, NSUserInterfaceLayoutOrientation, NSView, NSWindow,
    NSWindowStyleMask,
};
use objc2_foundation::{NSEdgeInsets, NSPoint, NSRect, NSSize, NSString};

use super::actions::Actions;
use super::widgets::{button, checkbox, heading, label, note, progress_bar, stack};
use crate::config::Config;
use crate::daemon::Controls;
use crate::stt::ModelId;

/// On/off settings shown as checkboxes. The checkbox tag is the index in `ALL`.
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

/// The controls of one model in the Speech model section. Button tags are
/// the model's index in `ModelId::ALL`.
pub struct ModelRow {
    model: ModelId,
    status: Retained<NSTextField>,
    progress: Retained<NSProgressIndicator>,
    download: Retained<NSButton>,
    choose: Retained<NSButton>,
    delete: Retained<NSButton>,
}

/// What the Speech model section shows, besides what's on disk.
pub struct ModelsState<'a> {
    pub current: ModelId,
    /// The model being downloaded and how far along it is (0 to 1).
    pub downloading: Option<(ModelId, f64)>,
    /// The last failed download and why.
    pub error: Option<&'a (ModelId, String)>,
}

/// Updates every model row to match what's installed and what's happening.
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

        let status = match (progress, error) {
            (Some(p), _) => format!("Downloading… {:.0}% of {size}", p * 100.0),
            (None, Some(e)) => format!("Download failed: {e}"),
            (None, None) if installed && current => "Downloaded · in use".into(),
            (None, None) if installed => "Downloaded".into(),
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

        row.choose
            .setTitle(&NSString::from_str(if current { "In use" } else { "Use" }));
        row.choose.setHidden(!installed);
        row.choose.setEnabled(!current);

        row.delete.setHidden(!installed);
        // The model in use can't be deleted; choose another one first.
        row.delete.setEnabled(!current);
    }
}

fn model_row(
    mtm: MainThreadMarker,
    target: &AnyObject,
    i: usize,
    model: ModelId,
) -> (Retained<NSStackView>, ModelRow) {
    let tag = i as isize;
    let name = label(mtm, model.label());
    name.setFont(Some(&objc2_app_kit::NSFont::boldSystemFontOfSize(12.0)));
    let row = ModelRow {
        model,
        status: note(mtm, ""),
        progress: progress_bar(mtm),
        download: button(mtm, "Download", target, sel!(downloadModel:), tag),
        choose: button(mtm, "Use", target, sel!(useModel:), tag),
        delete: button(mtm, "Delete", target, sel!(deleteModel:), tag),
    };
    let horizontal = NSUserInterfaceLayoutOrientation::Horizontal;
    let buttons = stack(
        mtm,
        horizontal,
        6.0,
        &[&row.download, &row.choose, &row.delete],
    );
    let title_line = stack(mtm, horizontal, 12.0, &[&name, &buttons]);
    row.progress
        .setFrameSize(objc2_foundation::NSSize::new(160.0, 12.0));
    let status_line = stack(mtm, horizontal, 8.0, &[&row.progress, &row.status]);
    let summary = note(mtm, model.summary());
    let column = stack(
        mtm,
        NSUserInterfaceLayoutOrientation::Vertical,
        3.0,
        &[&title_line, &summary, &status_line],
    );
    (column, row)
}

/// Builds the window. It's created once and hidden rather than released when
/// closed, so reopening it is instant.
pub fn build(mtm: MainThreadMarker, actions: &Actions) -> (Retained<NSWindow>, Vec<ModelRow>) {
    let target: &AnyObject = actions;
    let controls = actions.controls();

    let stack = NSStackView::new(mtm);
    stack.setOrientation(NSUserInterfaceLayoutOrientation::Vertical);
    stack.setAlignment(NSLayoutAttribute::Leading);
    stack.setSpacing(6.0);
    stack.setEdgeInsets(NSEdgeInsets {
        top: 20.0,
        left: 24.0,
        bottom: 24.0,
        right: 24.0,
    });

    let add = |view: &NSView, space_after: f64| {
        stack.addArrangedSubview(view);
        stack.setCustomSpacing_afterView(space_after, view);
    };
    add(&heading(mtm, "Speech model"), 10.0);
    let mut rows = Vec::new();
    for (i, model) in ModelId::ALL.into_iter().enumerate() {
        let (view, row) = model_row(mtm, target, i, model);
        add(&view, 14.0);
        rows.push(row);
    }
    let where_from = note(
        mtm,
        "Models download from Hugging Face through sayit's bundled script, and every \
         file is checked against a pinned SHA-256 checksum. After that, sayit works offline.",
    );
    where_from.setPreferredMaxLayoutWidth(420.0);
    add(&where_from, 24.0);

    add(&heading(mtm, "Text"), 10.0);
    for (i, toggle) in Toggle::ALL.into_iter().enumerate() {
        let on = toggle.flag(controls).load(Ordering::Relaxed);
        let action = sel!(toggleSetting:);
        add(
            &checkbox(mtm, toggle.title(), on, target, action, i as isize),
            2.0,
        );
        let explanation = note(mtm, toggle.explanation());
        explanation.setPreferredMaxLayoutWidth(400.0);
        add(&explanation, 14.0);
    }

    let frame = NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(460.0, 300.0));
    let style = NSWindowStyleMask::Titled | NSWindowStyleMask::Closable;
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
    window.setTitle(&NSString::from_str("sayit Settings"));
    window.setContentView(Some(&stack));
    window.setContentSize(stack.fittingSize());
    window.center();
    (window, rows)
}

/// Brings the window to the front. sayit has no Dock icon, so the app must
/// be activated explicitly or the window would open behind others.
pub fn show(mtm: MainThreadMarker, window: &NSWindow) {
    window.makeKeyAndOrderFront(None);
    // `activate` needs macOS 14; sayit supports 13.
    #[allow(deprecated)]
    NSApplication::sharedApplication(mtm).activateIgnoringOtherApps(true);
}
