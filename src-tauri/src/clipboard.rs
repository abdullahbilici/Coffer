//! Copies secrets to the clipboard and clears them after a while.

use std::{thread, time::Duration};

use objc2_app_kit::{NSPasteboard, NSPasteboardType, NSPasteboardTypeString};
use objc2_foundation::{NSData, NSInteger, NSString, ns_string};

/// The nspasteboard.org markers: clipboard history managers that honour them (Raycast, Paste,
/// Maccy, Alfred) don't record the item.
fn markers() -> [&'static NSPasteboardType; 2] {
    [
        ns_string!("org.nspasteboard.ConcealedType"),
        ns_string!("org.nspasteboard.TransientType"),
    ]
}

/// Copies a secret to the general pasteboard and, with `clear_after`, clears it later unless
/// anything was copied since. `concealed` adds the markers that keep it out of clipboard
/// history.
pub fn copy_secret(text: &str, concealed: bool, clear_after: Option<Duration>) {
    let copied = write(&NSPasteboard::generalPasteboard(), text, concealed);
    if let Some(delay) = clear_after {
        clear_later(copied, delay);
    }
}

/// Replaces the pasteboard's contents with `text`; returns the change count of this write.
fn write(pasteboard: &NSPasteboard, text: &str, concealed: bool) -> NSInteger {
    pasteboard.clearContents();
    // SAFETY: the type is an immutable static string.
    pasteboard.setString_forType(&NSString::from_str(text), unsafe { NSPasteboardTypeString });
    if concealed {
        for marker in markers() {
            pasteboard.setData_forType(Some(&NSData::new()), marker);
        }
    }
    pasteboard.changeCount()
}

fn clear_later(copied: NSInteger, delay: Duration) {
    thread::spawn(move || {
        thread::sleep(delay);
        let pasteboard = NSPasteboard::generalPasteboard();
        if pasteboard.changeCount() == copied {
            pasteboard.clearContents();
        }
    });
}

#[cfg(test)]
mod tests {
    use objc2::{msg_send, rc::Retained};

    use super::*;

    /// A private pasteboard, so tests never touch the user's clipboard.
    struct Private(Retained<NSPasteboard>);

    impl Drop for Private {
        fn drop(&mut self) {
            // SAFETY: releaseGlobally takes no arguments and returns nothing.
            let () = unsafe { msg_send![&*self.0, releaseGlobally] };
        }
    }

    fn types(pasteboard: &NSPasteboard) -> Vec<String> {
        pasteboard
            .types()
            .map(|types| types.iter().map(|t| t.to_string()).collect())
            .unwrap_or_default()
    }

    #[test]
    fn concealed_write_carries_the_markers() {
        let pasteboard = Private(NSPasteboard::pasteboardWithUniqueName());
        write(&pasteboard.0, "sk_test_123", true);

        let types = types(&pasteboard.0);
        for expected in [
            "public.utf8-plain-text",
            "org.nspasteboard.ConcealedType",
            "org.nspasteboard.TransientType",
        ] {
            assert!(
                types.iter().any(|t| t == expected),
                "{expected} in {types:?}"
            );
        }
        // SAFETY: the type is an immutable static string.
        let text = pasteboard
            .0
            .stringForType(unsafe { NSPasteboardTypeString });
        assert_eq!(text.map(|s| s.to_string()).as_deref(), Some("sk_test_123"));
    }

    #[test]
    fn plain_write_has_no_markers() {
        let pasteboard = Private(NSPasteboard::pasteboardWithUniqueName());
        write(&pasteboard.0, "sk_test_123", true);
        write(&pasteboard.0, "sk_test_456", false);

        let types = types(&pasteboard.0);
        assert!(types.iter().any(|t| t == "public.utf8-plain-text"));
        assert!(
            !types.iter().any(|t| t.starts_with("org.nspasteboard.")),
            "{types:?}"
        );
    }
}
