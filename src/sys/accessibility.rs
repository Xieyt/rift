use std::ffi::c_void;
use std::thread;
use std::time::{Duration, Instant};

use objc2::rc::autoreleasepool;
use objc2::runtime::AnyObject;
use objc2::{class, msg_send};
use tracing::info;

#[link(name = "ApplicationServices", kind = "framework")]
unsafe extern "C" {
    fn AXIsProcessTrustedWithOptions(options: *const c_void) -> bool;

    static kAXTrustedCheckOptionPrompt: *const c_void;
}

#[link(name = "CoreFoundation", kind = "framework")]
unsafe extern "C" {
    static kCFBooleanTrue: *const c_void;
    static kCFBooleanFalse: *const c_void;
}

const AX_POLL_INTERVAL: Duration = Duration::from_millis(250);
const AX_POLL_TIMEOUT: Duration = Duration::from_secs(30);

#[inline]
fn ax_is_trusted() -> bool {
    unsafe {
        autoreleasepool(|_| {
            let keys: [*mut AnyObject; 1] = [kAXTrustedCheckOptionPrompt as *mut AnyObject];
            let vals: [*mut AnyObject; 1] = [kCFBooleanFalse as *mut AnyObject];
            let dict: *mut AnyObject = msg_send![
                class!(NSDictionary),
                dictionaryWithObjects: vals.as_ptr(),
                forKeys:              keys.as_ptr(),
                count:                1usize
            ];

            AXIsProcessTrustedWithOptions(dict.cast())
        })
    }
}

#[allow(unsafe_op_in_unsafe_fn)]
unsafe fn prompt_ax_trust_dialog() {
    autoreleasepool(|_| {
        let keys: [*mut AnyObject; 1] = [kAXTrustedCheckOptionPrompt as *mut AnyObject];
        let vals: [*mut AnyObject; 1] = [kCFBooleanTrue as *mut AnyObject];

        let dict: *mut AnyObject = msg_send![
            class!(NSDictionary),
            dictionaryWithObjects: vals.as_ptr(),
            forKeys:              keys.as_ptr(),
            count:                1usize
        ];

        let _ = AXIsProcessTrustedWithOptions(dict.cast());
    });
}

pub fn ensure_accessibility_permission() {
    if ax_is_trusted() {
        return;
    }

    info!("Accessibility permission is not granted; prompting user for permission now.");

    unsafe { prompt_ax_trust_dialog() };

    let start = Instant::now();
    loop {
        if ax_is_trusted() {
            info!("Accessibility permission granted");
            return;
        }

        if start.elapsed() >= AX_POLL_TIMEOUT {
            break;
        }

        thread::sleep(AX_POLL_INTERVAL);
    }

    println!(
        "Rift still does not have accessibility permission. Enable it in System Settings > Privacy & Security > Accessibility, then restart Rift."
    );

    std::process::exit(1);
}

#[link(name = "CoreGraphics", kind = "framework")]
unsafe extern "C" {
    fn CGPreflightScreenCaptureAccess() -> bool;
    fn CGRequestScreenCaptureAccess() -> bool;
}

/// Window titles are only readable from WindowServer with this grant. Without
/// it `window_server::window_title` returns `None` for every window and the
/// title path falls back to a synchronous Accessibility request issued from
/// inside the app's own notification callback - measured at a 2.4ms median
/// against 0.15ms for the WindowServer query, on every title change.
pub fn ensure_screen_recording_permission() {
    if unsafe { CGPreflightScreenCaptureAccess() } {
        return;
    }

    info!("Screen Recording permission is not granted; prompting user for permission now.");

    // Only raises a dialog the first time this binary asks. Once TCC holds any
    // decision - including a stale one that no longer applies - this returns
    // without showing anything, which is why the failure text below names the
    // reset command instead of only pointing at the settings pane.
    unsafe { CGRequestScreenCaptureAccess() };

    let start = Instant::now();
    loop {
        if unsafe { CGPreflightScreenCaptureAccess() } {
            info!("Screen Recording permission granted");
            return;
        }

        if start.elapsed() >= AX_POLL_TIMEOUT {
            break;
        }

        thread::sleep(AX_POLL_INTERVAL);
    }

    println!(
        "Rift needs Screen Recording to read window titles from the window server.\n\
         Without it every title change costs ~2.4ms instead of ~0.15ms.\n\
         \n\
         Grant it in System Settings > Privacy & Security >\n\
         \x20 Screen & System Audio Recording, then restart Rift.\n\
         \n\
         If Rift is ALREADY listed and switched on, the grant has gone stale.\n\
         Clear it and relaunch to get a fresh prompt:\n\
         \n\
         \x20   tccutil reset ScreenCapture git.acsandmann.rift"
    );

    std::process::exit(1);
}
