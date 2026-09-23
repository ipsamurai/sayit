//! The two permissions sayit needs, for the setup checklist: reading their
//! status and asking for them. Nothing here grants anything by itself; macOS
//! asks the user.

use block2::RcBlock;
use objc2::msg_send;
use objc2::runtime::{AnyClass, Bool};
use objc2_app_kit::NSWorkspace;
use objc2_foundation::{NSString, NSURL};

use crate::daemon;

#[derive(Clone, Copy, PartialEq)]
pub enum Access {
    Allowed,
    /// Never asked: requesting shows the macOS prompt.
    NotAsked,
    /// Refused (or blocked by a profile): only System Settings can change it.
    Denied,
}

pub fn accessibility() -> Access {
    if daemon::accessibility_trusted(false) {
        Access::Allowed
    } else {
        Access::NotAsked
    }
}

/// Shows the macOS Accessibility prompt and opens the matching pane of
/// System Settings, where the user turns sayit on.
pub fn request_accessibility() {
    daemon::accessibility_trusted(true);
    open_privacy_pane("Privacy_Accessibility");
}

#[link(name = "AVFoundation", kind = "framework")]
unsafe extern "C" {
    static AVMediaTypeAudio: &'static NSString;
}

fn capture_device() -> Option<&'static AnyClass> {
    AnyClass::get(c"AVCaptureDevice")
}

pub fn microphone() -> Access {
    let Some(class) = capture_device() else {
        return Access::NotAsked;
    };
    // SAFETY: +[AVCaptureDevice authorizationStatusForMediaType:] takes an
    // AVMediaType (a constant NSString) and returns an NSInteger.
    let status: isize =
        unsafe { msg_send![class, authorizationStatusForMediaType: AVMediaTypeAudio] };
    match status {
        3 => Access::Allowed,  // AVAuthorizationStatusAuthorized
        0 => Access::NotAsked, // AVAuthorizationStatusNotDetermined
        _ => Access::Denied,   // Denied or Restricted
    }
}

/// First time: the macOS microphone prompt. After a refusal, macOS won't ask
/// again, so this opens System Settings instead.
pub fn request_microphone() {
    if microphone() != Access::NotAsked {
        open_privacy_pane("Privacy_Microphone");
        return;
    }
    let Some(class) = capture_device() else {
        return;
    };
    // The checklist polls the status, so the answer itself isn't needed.
    let done = RcBlock::new(|_granted: Bool| {});
    // SAFETY: +[AVCaptureDevice requestAccessForMediaType:completionHandler:]
    // takes an AVMediaType and a `void (^)(BOOL)` block, which it copies.
    let _: () = unsafe {
        msg_send![class, requestAccessForMediaType: AVMediaTypeAudio, completionHandler: &*done]
    };
}

fn open_privacy_pane(anchor: &str) {
    let url = format!("x-apple.systempreferences:com.apple.preference.security?{anchor}");
    if let Some(url) = NSURL::URLWithString(&NSString::from_str(&url)) {
        NSWorkspace::sharedWorkspace().openURL(&url);
    }
}
