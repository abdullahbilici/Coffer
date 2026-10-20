use std::sync::mpsc;

use block2::RcBlock;
use objc2::runtime::Bool;
use objc2_foundation::{NSError, NSString};
use objc2_local_authentication::{LAContext, LAPolicy};

use crate::error::{AppError, AppResult};

/// Prompts the Mac's owner authentication (Touch ID, falling back to the account password).
/// Blocks until the user responds, so call it off the main thread.
pub fn authenticate(reason: &str) -> AppResult<()> {
    let (tx, rx) = mpsc::channel();
    let context = unsafe { LAContext::new() };
    let reason = NSString::from_str(reason);

    let reply = RcBlock::new(move |success: Bool, error: *mut NSError| {
        let result = if success.as_bool() {
            Ok(())
        } else {
            Err(unsafe { error.as_ref() }
                .map(|e| e.localizedDescription().to_string())
                .unwrap_or_else(|| "Authentication failed".to_string()))
        };
        let _ = tx.send(result);
    });

    unsafe {
        context.evaluatePolicy_localizedReason_reply(
            LAPolicy::DeviceOwnerAuthentication,
            &reason,
            &reply,
        );
    }

    rx.recv()
        .map_err(|_| AppError::Auth("Authentication was interrupted".to_string()))?
        .map_err(AppError::Auth)
}
