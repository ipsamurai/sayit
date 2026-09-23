//! Small constructors for AppKit controls, so the UI code reads as layout.

use objc2::rc::Retained;
use objc2::runtime::{AnyObject, Sel};
use objc2::{MainThreadMarker, MainThreadOnly};
use objc2_app_kit::{
    NSBox, NSBoxType, NSButton, NSColor, NSControlSize, NSControlStateValueOff,
    NSControlStateValueOn, NSFont, NSFontWeightMedium, NSFontWeightSemibold, NSImage,
    NSImageSymbolConfiguration, NSImageView, NSLayoutAttribute, NSLayoutConstraintOrientation,
    NSMenuItem, NSPopUpButton, NSProgressIndicator, NSProgressIndicatorStyle,
    NSSegmentSwitchTracking, NSSegmentedControl, NSStackView, NSStackViewDistribution, NSSwitch,
    NSTextAlignment, NSTextField, NSTitlePosition, NSUserInterfaceLayoutOrientation, NSView,
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

/// Pins a view's height, so rows keep their size when controls in them are
/// shown or hidden.
pub fn fixed_height(view: &NSView, height: f64) {
    view.heightAnchor()
        .constraintEqualToConstant(height)
        .setActive(true);
}

/// An empty view that takes up the free space in a row, pushing the views
/// after it to the far end.
pub fn spacer(mtm: MainThreadMarker) -> Retained<NSView> {
    let view = NSView::new(mtm);
    view.setContentHuggingPriority_forOrientation(1.0, NSLayoutConstraintOrientation::Horizontal);
    view
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

/// Text centred in its column, e.g. a page title or subtitle.
pub fn centered(label: &NSTextField) {
    label.setAlignment(NSTextAlignment::Center);
}

/// An SF Symbol at `size` points, tinted `color`.
pub fn symbol(
    mtm: MainThreadMarker,
    name: &str,
    size: f64,
    color: &NSColor,
) -> Retained<NSImageView> {
    let view = NSImageView::new(mtm);
    set_symbol(&view, name, size, color);
    fixed_width(&view, size + 8.0);
    fixed_height(&view, size + 8.0);
    view
}

/// Changes the symbol shown by an image view made with `symbol`.
pub fn set_symbol(view: &NSImageView, name: &str, size: f64, color: &NSColor) {
    let image = NSImage::imageWithSystemSymbolName_accessibilityDescription(
        &NSString::from_str(name),
        None,
    );
    view.setImage(image.as_deref());
    // SAFETY: reading an immutable AppKit constant that is set at load time.
    let weight = unsafe { NSFontWeightMedium };
    let config = NSImageSymbolConfiguration::configurationWithPointSize_weight(size, weight);
    view.setSymbolConfiguration(Some(&config));
    view.setContentTintColor(Some(color));
}

/// A symbol on a softly tinted rounded square, like the icons in System
/// Settings.
pub fn icon_badge(
    mtm: MainThreadMarker,
    name: &str,
    size: f64,
    color: &NSColor,
) -> Retained<NSBox> {
    let badge = NSBox::new(mtm);
    badge.setBoxType(NSBoxType::Custom);
    badge.setTitlePosition(NSTitlePosition::NoTitle);
    badge.setBorderWidth(0.0);
    badge.setCornerRadius(size * 0.28);
    badge.setFillColor(&color.colorWithAlphaComponent(0.16));
    badge.setContentViewMargins(NSSize::new(0.0, 0.0));
    let icon = symbol(mtm, name, size * 0.5, color);
    let holder = stack(
        mtm,
        NSUserInterfaceLayoutOrientation::Vertical,
        0.0,
        &[&icon],
    );
    holder.setAlignment(NSLayoutAttribute::CenterX);
    badge.setContentView(Some(&holder));
    fixed_width(&badge, size);
    fixed_height(&badge, size);
    badge
}

/// A pop-up menu of `titles`; changes go to `target`'s `action`.
pub fn popup(
    mtm: MainThreadMarker,
    titles: &[&str],
    selected: usize,
    target: &AnyObject,
    action: Sel,
) -> Retained<NSPopUpButton> {
    let menu = NSPopUpButton::initWithFrame_pullsDown(
        NSPopUpButton::alloc(mtm),
        objc2_foundation::NSRect::ZERO,
        false,
    );
    for title in titles {
        menu.addItemWithTitle(&NSString::from_str(title));
    }
    menu.selectItemAtIndex(selected as isize);
    // SAFETY: `action` is implemented by `target`, which outlives the window.
    unsafe {
        menu.setTarget(Some(target));
        menu.setAction(Some(action));
    }
    menu
}

/// Side-by-side choices (one selectable); changes go to `target`'s `action`.
pub fn segmented(
    mtm: MainThreadMarker,
    labels: &[&str],
    selected: usize,
    target: &AnyObject,
    action: Sel,
) -> Retained<NSSegmentedControl> {
    let labels: Vec<_> = labels.iter().map(|l| NSString::from_str(l)).collect();
    let labels = objc2_foundation::NSArray::from_retained_slice(&labels);
    // SAFETY: `action` is implemented by `target`, which outlives the window.
    let control = unsafe {
        NSSegmentedControl::segmentedControlWithLabels_trackingMode_target_action(
            &labels,
            NSSegmentSwitchTracking::SelectOne,
            Some(target),
            Some(action),
            mtm,
        )
    };
    control.setSelectedSegment(selected as isize);
    control
}

/// A row in a card: optional leading icon, a title with one short line under
/// it, then an optional control at the far right.
pub fn card_row(
    mtm: MainThreadMarker,
    leading: Option<&NSView>,
    title: &NSTextField,
    detail: &NSTextField,
    trailing: Option<&NSView>,
    width: f64,
) -> Retained<NSStackView> {
    let texts = stack(
        mtm,
        NSUserInterfaceLayoutOrientation::Vertical,
        2.0,
        &[title, detail],
    );
    let mut views: Vec<&NSView> = Vec::new();
    if let Some(leading) = leading {
        views.push(leading);
    }
    let gap = spacer(mtm);
    views.extend([&*texts as &NSView, &gap]);
    if let Some(trailing) = trailing {
        views.push(trailing);
    }
    let row = stack(
        mtm,
        NSUserInterfaceLayoutOrientation::Horizontal,
        12.0,
        &views,
    );
    row.setDistribution(NSStackViewDistribution::Fill);
    fixed_width(&row, width);
    row
}

/// Stacks rows in a card with thin separators between them.
pub fn rows_card(mtm: MainThreadMarker, rows: &[&NSView], width: f64) -> Retained<NSBox> {
    let inner = width - 28.0;
    let column = NSStackView::new(mtm);
    column.setOrientation(NSUserInterfaceLayoutOrientation::Vertical);
    column.setSpacing(10.0);
    for (i, row) in rows.iter().enumerate() {
        if i > 0 {
            column.addArrangedSubview(&separator(mtm, inner));
        }
        column.addArrangedSubview(row);
    }
    card(mtm, &column, width)
}
