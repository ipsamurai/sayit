//! The Settings window. Changes apply immediately and are saved to config.toml.

use std::sync::atomic::{AtomicBool, Ordering};

use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::{MainThreadMarker, MainThreadOnly, sel};
use objc2_app_kit::{
    NSApplication, NSBackingStoreType, NSLayoutAttribute, NSStackView,
    NSUserInterfaceLayoutOrientation, NSView, NSWindow, NSWindowStyleMask,
};
use objc2_foundation::{NSEdgeInsets, NSPoint, NSRect, NSSize, NSString};

use super::actions::Actions;
use super::widgets::{checkbox, heading, note};
use crate::config::Config;
use crate::daemon::Controls;

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

/// Builds the window. It's created once and hidden rather than released when
/// closed, so reopening it is instant.
pub fn build(mtm: MainThreadMarker, actions: &Actions) -> Retained<NSWindow> {
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
    window.center();
    window
}

/// Brings the window to the front. sayit has no Dock icon, so the app must
/// be activated explicitly or the window would open behind others.
pub fn show(mtm: MainThreadMarker, window: &NSWindow) {
    window.makeKeyAndOrderFront(None);
    // `activate` needs macOS 14; sayit supports 13.
    #[allow(deprecated)]
    NSApplication::sharedApplication(mtm).activateIgnoringOtherApps(true);
}
