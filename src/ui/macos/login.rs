//! "Start at login", through macOS's own login-items service (SMAppService,
//! macOS 13+). It needs no entitlement, and the user can always turn it off
//! in System Settings › General › Login Items.

use objc2::msg_send;
use objc2::rc::Retained;
use objc2::runtime::{AnyClass, AnyObject};
use objc2_foundation::NSError;

use crate::paths;

#[link(name = "ServiceManagement", kind = "framework")]
unsafe extern "C" {}

#[derive(Clone, Copy, PartialEq)]
pub enum Login {
    /// Only the installed sayit.app can be a login item, not a binary run
    /// from a terminal.
    Unavailable,
    Off,
    On,
    /// Turned on, but macOS wants the user to allow it in System Settings.
    NeedsApproval,
}

/// The login item for sayit.app itself.
fn service() -> Option<Retained<AnyObject>> {
    if !paths::in_app_bundle() {
        return None;
    }
    let class = AnyClass::get(c"SMAppService")?;
    // SAFETY: +[SMAppService mainAppService] takes no arguments and returns
    // an SMAppService object.
    unsafe { msg_send![class, mainAppService] }
}

pub fn status() -> Login {
    let Some(service) = service() else {
        return Login::Unavailable;
    };
    // SAFETY: -[SMAppService status] returns an SMAppServiceStatus (NSInteger).
    let status: isize = unsafe { msg_send![&service, status] };
    match status {
        1 => Login::On,            // SMAppServiceStatusEnabled
        2 => Login::NeedsApproval, // SMAppServiceStatusRequiresApproval
        _ => Login::Off,           // NotRegistered or NotFound
    }
}

/// Adds or removes sayit from the login items, returning macOS's reason if
/// it refused.
pub fn set(on: bool) -> Result<(), String> {
    let service = service().ok_or("only the installed sayit.app can start at login")?;
    // SAFETY: -registerAndReturnError: and -unregisterAndReturnError: take an
    // NSError out-pointer and return BOOL; `_` turns that into a Result.
    let result: Result<(), Retained<NSError>> = unsafe {
        if on {
            msg_send![&service, registerAndReturnError: _]
        } else {
            msg_send![&service, unregisterAndReturnError: _]
        }
    };
    result.map_err(|e| e.localizedDescription().to_string())
}
