//! Small constructors for AppKit controls, so the UI code reads as layout.

use objc2::rc::Retained;
use objc2::runtime::{AnyObject, Sel};
use objc2::{MainThreadMarker, MainThreadOnly};
use objc2_app_kit::{
    NSBox, NSBoxType, NSButton, NSColor, NSControlSize, NSControlStateValueOff,
    NSControlStateValueOn, NSFont, NSFontWeightSemibold, NSLayoutAttribute, NSMenuItem,
    NSProgressIndicator, NSProgressIndicatorStyle, NSStackView, NSSwitch, NSTextField,
    NSTitlePosition, NSUserInterfaceLayoutOrientation, NSView,
};
use objc2_foundation::{NSSize, NSString};

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

/// An on/off switch whose changes go to `target`'s `action`; `tag` tells
/// them apart.
pub fn switch(
    mtm: MainThreadMarker,
    on: bool,
    target: &AnyObject,
    action: Sel,
    tag: isize,
) -> Retained<NSSwitch> {
    let switch = NSSwitch::new(mtm);
    switch.setControlSize(NSControlSize::Small);
    switch.setState(check_state(on));
    switch.setTag(tag);
    // SAFETY: `action` is implemented by `target`, which outlives the window.
    unsafe {
        switch.setTarget(Some(target));
        switch.setAction(Some(action));
    }
    switch
}

/// A rounded, subtly filled panel around `content`, `width` points wide.
pub fn card(mtm: MainThreadMarker, content: &NSView, width: f64) -> Retained<NSBox> {
    let card = NSBox::new(mtm);
    card.setBoxType(NSBoxType::Custom);
    card.setTitlePosition(NSTitlePosition::NoTitle);
    card.setCornerRadius(10.0);
    card.setBorderColor(&NSColor::separatorColor());
    card.setFillColor(&NSColor::quaternarySystemFillColor());
    card.setContentViewMargins(NSSize::new(14.0, 14.0));
    card.setContentView(Some(content));
    fixed_width(&card, width);
    card
}

/// A thin horizontal line between rows in a card.
pub fn separator(mtm: MainThreadMarker, width: f64) -> Retained<NSBox> {
    let line = NSBox::new(mtm);
    line.setBoxType(NSBoxType::Separator);
    fixed_width(&line, width);
    line
}

/// Pins a view's width, so wrapping text inside it lays out predictably.
pub fn fixed_width(view: &NSView, width: f64) {
    view.widthAnchor()
        .constraintEqualToConstant(width)
        .setActive(true);
}

/// Semibold 13 pt text for item names, as in System Settings.
pub fn semibold(mtm: MainThreadMarker, text: &str) -> Retained<NSTextField> {
    let label = NSTextField::labelWithString(&NSString::from_str(text), mtm);
    // SAFETY: reading an immutable AppKit constant that is set at load time.
    let weight = unsafe { NSFontWeightSemibold };
    label.setFont(Some(&NSFont::systemFontOfSize_weight(13.0, weight)));
    label
}

/// Secondary explanatory text that wraps at `width` points.
pub fn note(mtm: MainThreadMarker, text: &str, width: f64) -> Retained<NSTextField> {
    let label = NSTextField::wrappingLabelWithString(&NSString::from_str(text), mtm);
    label.setFont(Some(&NSFont::systemFontOfSize(11.0)));
    label.setTextColor(Some(&NSColor::secondaryLabelColor()));
    label.setPreferredMaxLayoutWidth(width);
    fixed_width(&label, width);
    label
}

/// A push button whose clicks go to `target`'s `action`; `tag` tells them apart.
pub fn button(
    mtm: MainThreadMarker,
    title: &str,
    target: &AnyObject,
    action: Sel,
    tag: isize,
) -> Retained<NSButton> {
    // SAFETY: `action` is implemented by `target`, which outlives the window.
    let button = unsafe {
        NSButton::buttonWithTitle_target_action(
            &NSString::from_str(title),
            Some(target),
            Some(action),
            mtm,
        )
    };
    button.setControlSize(NSControlSize::Small);
    button.setTag(tag);
    button
}

pub fn label(mtm: MainThreadMarker, text: &str) -> Retained<NSTextField> {
    NSTextField::labelWithString(&NSString::from_str(text), mtm)
}

/// A determinate progress bar from 0 to 1.
pub fn progress_bar(mtm: MainThreadMarker) -> Retained<NSProgressIndicator> {
    let bar = NSProgressIndicator::new(mtm);
    bar.setStyle(NSProgressIndicatorStyle::Bar);
    bar.setIndeterminate(false);
    bar.setMinValue(0.0);
    bar.setMaxValue(1.0);
    bar
}

/// Lays views out in a row (`Horizontal`) or a column (`Vertical`).
pub fn stack(
    mtm: MainThreadMarker,
    orientation: NSUserInterfaceLayoutOrientation,
    spacing: f64,
    views: &[&NSView],
) -> Retained<NSStackView> {
    let stack = NSStackView::new(mtm);
    stack.setOrientation(orientation);
    stack.setSpacing(spacing);
    if orientation == NSUserInterfaceLayoutOrientation::Vertical {
        stack.setAlignment(NSLayoutAttribute::Leading);
    } else {
        stack.setAlignment(NSLayoutAttribute::CenterY);
    }
    for view in views {
        stack.addArrangedSubview(view);
    }
    stack
}
