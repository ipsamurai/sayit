//! The setup assistant shown on first launch: welcome, choose and download a
//! model, then a short how-to. It has no close button; finishing it saves
//! `setup_complete` and starts dictation.

use std::cell::Cell;

use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::{MainThreadMarker, MainThreadOnly, sel};
use objc2_app_kit::{
    NSApplication, NSBackingStoreType, NSButton, NSColor, NSFont, NSImage,
    NSImageSymbolConfiguration, NSImageView, NSLayoutAttribute, NSStackView,
    NSStackViewDistribution, NSTabView, NSTabViewItem, NSTabViewType, NSTextField,
    NSUserInterfaceLayoutOrientation, NSView, NSWindow, NSWindowStyleMask,
};
use objc2_foundation::{NSPoint, NSRect, NSSize, NSString};

use super::actions::Actions;
use super::settings::{ModelRow, PANE_WIDTH, model_card, pane};
use super::widgets::{
    button, fixed_height, fixed_width, label, note, semibold, separator, spacer, stack,
};
use crate::daemon::Controls;
use crate::stt::ModelId;

/// The assistant's pages, in order.
#[derive(Clone, Copy, PartialEq)]
enum Page {
    Welcome,
    Model,
    Done,
}

const PAGES: [Page; 3] = [Page::Welcome, Page::Model, Page::Done];

pub struct Setup {
    pub window: Retained<NSWindow>,
    pages: Retained<NSTabView>,
    current: Cell<usize>,
    step: Retained<NSTextField>,
    back: Retained<NSButton>,
    next: Retained<NSButton>,
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
        let last = index == PAGES.len() - 1;
        let title = if last { "Finish" } else { "Continue" };
        self.next.setTitle(&NSString::from_str(title));
        self.refresh(controls);
    }

    /// Continue is only enabled once the current page is complete.
    pub fn refresh(&self, controls: &Controls) {
        let ready = match PAGES[self.current.get()] {
            Page::Model => controls.model().is_installed(),
            Page::Welcome | Page::Done => true,
        };
        self.next.setEnabled(ready);
    }

    pub fn current(&self) -> usize {
        self.current.get()
    }

    pub fn is_last(&self) -> bool {
        self.current.get() == PAGES.len() - 1
    }
}

/// Builds the assistant window; the model cards are returned so they can be
/// refreshed with the ones in Settings.
pub fn build(mtm: MainThreadMarker, actions: &Actions, hotkey: &str) -> (Setup, Vec<ModelRow>) {
    let target: &AnyObject = actions;
    let (model_page, rows) = model_page(mtm, target);
    let page_views = [welcome_page(mtm), model_page, done_page(mtm, hotkey)];

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
    let width = PANE_WIDTH + 48.0;
    fixed_width(&pages, width);
    fixed_height(&pages, height);

    let step = label(mtm, "");
    step.setFont(Some(&NSFont::systemFontOfSize(11.0)));
    step.setTextColor(Some(&NSColor::secondaryLabelColor()));
    let back = button(mtm, "Back", target, sel!(setupBack:), 0);
    let next = button(mtm, "Continue", target, sel!(setupNext:), 0);
    for b in [&back, &next] {
        b.setControlSize(objc2_app_kit::NSControlSize::Regular);
    }
    // Return presses Continue, as in other Mac assistants.
    next.setKeyEquivalent(&NSString::from_str("\r"));
    let horizontal = NSUserInterfaceLayoutOrientation::Horizontal;
    let bar = stack(mtm, horizontal, 8.0, &[&step, &spacer(mtm), &back, &next]);
    bar.setDistribution(NSStackViewDistribution::Fill);
    bar.setEdgeInsets(objc2_foundation::NSEdgeInsets {
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
    // No Closable: setup has to be finished (⌘Q still quits sayit).
    let style = NSWindowStyleMask::Titled;
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
    };
    (setup, rows)
}

/// Brings the assistant to the front (sayit has no Dock icon).
pub fn show(mtm: MainThreadMarker, setup: &Setup) {
    setup.window.makeKeyAndOrderFront(None);
    // `activate` needs macOS 14; sayit supports 13.
    #[allow(deprecated)]
    NSApplication::sharedApplication(mtm).activateIgnoringOtherApps(true);
}

fn heading(mtm: MainThreadMarker, text: &str) -> Retained<NSTextField> {
    let field = label(mtm, text);
    // SAFETY: reading an immutable AppKit constant that is set at load time.
    let weight = unsafe { objc2_app_kit::NSFontWeightSemibold };
    field.setFont(Some(&NSFont::systemFontOfSize_weight(22.0, weight)));
    field
}

fn subtitle(mtm: MainThreadMarker, text: &str) -> Retained<NSTextField> {
    let field = note(mtm, text, PANE_WIDTH);
    field.setFont(Some(&NSFont::systemFontOfSize(13.0)));
    field
}

/// An SF Symbol drawn at `size` points in the accent color.
fn symbol(mtm: MainThreadMarker, name: &str, size: f64) -> Retained<NSImageView> {
    let image = NSImage::imageWithSystemSymbolName_accessibilityDescription(
        &NSString::from_str(name),
        None,
    )
    .unwrap_or_default();
    let view = NSImageView::imageViewWithImage(&image, mtm);
    // SAFETY: reading an immutable AppKit constant that is set at load time.
    let weight = unsafe { objc2_app_kit::NSFontWeightRegular };
    let config = NSImageSymbolConfiguration::configurationWithPointSize_weight(size, weight);
    view.setSymbolConfiguration(Some(&config));
    view.setContentTintColor(Some(&NSColor::controlAccentColor()));
    fixed_width(&view, size + 12.0);
    view
}

/// An icon, a bold line and an explanation, e.g. on the welcome page.
fn feature(mtm: MainThreadMarker, icon: &str, title: &str, text: &str) -> Retained<NSStackView> {
    let texts = stack(
        mtm,
        NSUserInterfaceLayoutOrientation::Vertical,
        2.0,
        &[&semibold(mtm, title), &note(mtm, text, PANE_WIDTH - 60.0)],
    );
    let row = stack(
        mtm,
        NSUserInterfaceLayoutOrientation::Horizontal,
        14.0,
        &[&symbol(mtm, icon, 22.0), &texts],
    );
    row.setAlignment(NSLayoutAttribute::Top);
    row
}

fn welcome_page(mtm: MainThreadMarker) -> Retained<NSStackView> {
    let icon = NSApplication::sharedApplication(mtm)
        .applicationIconImage()
        .map(|image| NSImageView::imageViewWithImage(&image, mtm));
    let title = heading(mtm, "Welcome to sayit");
    let intro = subtitle(
        mtm,
        "Private dictation that runs entirely on your Mac. Setup takes a minute.",
    );
    let features = [
        feature(
            mtm,
            "lock.shield",
            "Private",
            "What you say never leaves this Mac. No account, no tracking, no cloud.",
        ),
        feature(
            mtm,
            "keyboard",
            "Works in any app",
            "Hold a key, speak, release, and your words are typed where your cursor is.",
        ),
        feature(
            mtm,
            "bolt",
            "Light",
            "Runs on the CPU, with no GPU needed, using about 1 GB of memory.",
        ),
    ];
    let legal = note(
        mtm,
        "sayit is free, open-source software provided as is. See DISCLAIMER.md and \
         PRIVACY.md in its GitHub repository.",
        PANE_WIDTH,
    );

    let mut refs: Vec<&NSView> = Vec::new();
    if let Some(icon) = &icon {
        fixed_width(icon, 72.0);
        fixed_height(icon, 72.0);
        refs.push(icon);
    }
    refs.extend([&*title as &NSView, &intro]);
    refs.extend(features.iter().map(|f| -> &NSView { f }));
    refs.push(&legal);
    let page = pane(mtm, &refs);
    page.setSpacing(16.0);
    page
}

fn model_page(mtm: MainThreadMarker, target: &AnyObject) -> (Retained<NSStackView>, Vec<ModelRow>) {
    let title = heading(mtm, "Choose a speech model");
    let intro = subtitle(
        mtm,
        "Download one to start. Parakeet v2 is the best choice for most Macs; you can \
         add or switch models later in Settings.",
    );
    let (cards, rows): (Vec<_>, Vec<_>) = ModelId::ALL
        .into_iter()
        .enumerate()
        .map(|(i, model)| model_card(mtm, target, i, model))
        .unzip();
    let mut refs: Vec<&NSView> = vec![&title, &intro];
    refs.extend(cards.iter().map(|card| -> &NSView { card }));
    (pane(mtm, &refs), rows)
}

fn done_page(mtm: MainThreadMarker, hotkey: &str) -> Retained<NSStackView> {
    let title = heading(mtm, "You're all set");
    let intro = subtitle(mtm, "Here's how to dictate anywhere:");
    let hold = format!("Hold {hotkey} and speak");
    let steps = [
        (
            "cursorarrow.click",
            "Click where you want to type",
            "Any app, any text field.",
        ),
        (
            "mic",
            hold.as_str(),
            "Keep holding while you talk. The menu-bar icon fills in while it listens.",
        ),
        (
            "text.cursor",
            "Release, and your words appear",
            "sayit types them at your cursor. Filler words like \"um\" are removed.",
        ),
        (
            "menubar.rectangle",
            "sayit lives in the menu bar",
            "Click its icon to pause, pick a microphone, or open Settings.",
        ),
    ];
    let rows: Vec<_> = steps
        .into_iter()
        .map(|(icon, title, text)| feature(mtm, icon, title, text))
        .collect();
    let mut refs: Vec<&NSView> = vec![&title, &intro];
    refs.extend(rows.iter().map(|row| -> &NSView { row }));
    let page = pane(mtm, &refs);
    page.setSpacing(16.0);
    page
}
