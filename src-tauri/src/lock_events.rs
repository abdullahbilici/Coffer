//! Closes the unlock window when the screen locks or the Mac goes to sleep.
//! The user stays in the app; only revealing secrets asks for Touch ID again.

use std::ptr::NonNull;

use block2::RcBlock;
use objc2_app_kit::{
    NSWorkspace, NSWorkspaceScreensDidSleepNotification, NSWorkspaceWillSleepNotification,
};
use objc2_foundation::{NSDistributedNotificationCenter, NSNotification, NSString};
use tauri::{AppHandle, Emitter, Manager};

use crate::{
    backup::{self, Trigger},
    state::AppState,
};

/// Emitted with a `SessionInfo` payload whenever the backend changes the session on its own.
pub const SESSION_CHANGED: &str = "session-changed";

const SCREEN_LOCKED: &str = "com.apple.screenIsLocked";

pub fn observe(app: &AppHandle) {
    let app = app.clone();
    let handler = RcBlock::new(move |_: NonNull<NSNotification>| {
        // Write pending changes while the key is still here. Failures are recorded in the
        // settings and shown in the Backup section.
        let _ = backup::run(&app, Trigger::Flush);
        let info = app.state::<AppState>().session().close_window();
        let _ = app.emit(SESSION_CHANGED, info);
    });

    // The observer tokens are leaked on purpose: they live as long as the app.
    unsafe {
        let workspace = NSWorkspace::sharedWorkspace().notificationCenter();
        for name in [
            NSWorkspaceWillSleepNotification,
            NSWorkspaceScreensDidSleepNotification,
        ] {
            std::mem::forget(workspace.addObserverForName_object_queue_usingBlock(
                Some(name),
                None,
                None,
                &handler,
            ));
        }

        let screen_locked = NSString::from_str(SCREEN_LOCKED);
        std::mem::forget(
            NSDistributedNotificationCenter::defaultCenter()
                .addObserverForName_object_queue_usingBlock(
                    Some(&screen_locked),
                    None,
                    None,
                    &handler,
                ),
        );
    }
}
