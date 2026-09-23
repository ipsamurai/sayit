//! Small constructors for AppKit controls, so the UI code reads as layout.

use objc2::rc::Retained;
use objc2::runtime::{AnyObject, Sel};
use objc2::{MainThreadMarker, MainThreadOnly};
use objc2_app_kit::{
    NSButton, NSColor, NSControlStateValueOff, NSControlStateValueOn, NSFont, NSMenuItem,
    NSTextField,
};
use objc2_foundation::NSString;

pub fn menu_item(
    mtm: MainThreadMarker,
    title: &str,
    action: Option<Sel>,
    key: &str,
) -> Retained<NSMenuItem> {
    // SAFETY: `action` is either None or a selector implemented by the item's
    // target (the Actions object) or by the responder chain (terminate:,
    // performClose:, copy: ...).
    unsafe {
        NSMenuItem::initWithTitle_action_keyEquivalent(
            NSMenuItem::alloc(mtm),
            &NSString::from_str(title),
            action,
            &NSString::from_str(key),
        )
    }
}

pub fn check_state(on: bool) -> isize {
    if on {
        NSControlStateValueOn
    } else {
        NSControlStateValueOff
    }
}

/// A checkbox whose clicks go to `target`'s `action`; `tag` tells them apart.
pub fn checkbox(
    mtm: MainThreadMarker,
    title: &str,
    on: bool,
    target: &AnyObject,
    action: Sel,
    tag: isize,
) -> Retained<NSButton> {
    // SAFETY: `action` is implemented by `target`, which outlives the window.
    let button = unsafe {
        NSButton::checkboxWithTitle_target_action(
            &NSString::from_str(title),
            Some(target),
            Some(action),
            mtm,
        )
    };
    button.setState(check_state(on));
    button.setTag(tag);
    button
}

pub fn heading(mtm: MainThreadMarker, text: &str) -> Retained<NSTextField> {
    let label = NSTextField::labelWithString(&NSString::from_str(text), mtm);
    label.setFont(Some(&NSFont::boldSystemFontOfSize(13.0)));
    label
}

/// Secondary explanatory text under a control.
pub fn note(mtm: MainThreadMarker, text: &str) -> Retained<NSTextField> {
    let label = NSTextField::wrappingLabelWithString(&NSString::from_str(text), mtm);
    label.setFont(Some(&NSFont::systemFontOfSize(11.0)));
    label.setTextColor(Some(&NSColor::secondaryLabelColor()));
    label
}
