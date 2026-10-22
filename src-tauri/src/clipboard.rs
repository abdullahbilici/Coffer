//! Clears copied secrets from the clipboard after a while.

use std::{thread, time::Duration};

use objc2_app_kit::NSPasteboard;

/// Clears the clipboard after `delay`, unless anything was copied since this call. Call it
/// right after the frontend wrote the secret, so the change count still matches that write.
pub fn clear_later(delay: Duration) {
    let copied = NSPasteboard::generalPasteboard().changeCount();
    thread::spawn(move || {
        thread::sleep(delay);
        let pasteboard = NSPasteboard::generalPasteboard();
        if pasteboard.changeCount() == copied {
            pasteboard.clearContents();
        }
    });
}
