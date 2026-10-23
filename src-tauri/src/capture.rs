//! Quick capture: a global shortcut shows a small panel over whatever app is in front, and a
//! line typed there goes to the assistant as a new conversation.
//!
//! The panel is created hidden at launch and only shown and hidden after that, so it opens at
//! once. It is a non-activating `NSPanel`: it takes the keyboard without activating Coffer, so
//! the main window stays where it is and hiding the panel leaves the user in the app they were
//! in. The shortcut only works while Coffer runs; there is no background mode.

use std::str::FromStr;

use objc2::{ClassType, MainThreadOnly, define_class, runtime::AnyObject};
use objc2_app_kit::{
    NSFloatingWindowLevel, NSPanel, NSResponder, NSWindow, NSWindowCollectionBehavior,
    NSWindowStyleMask,
};
use objc2_foundation::NSObject;
use tauri::{
    AppHandle, Emitter, LogicalSize, Manager, PhysicalPosition, WebviewUrl, WebviewWindow,
    WebviewWindowBuilder, Wry, plugin::TauriPlugin,
};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Modifiers, Shortcut, ShortcutState};

use crate::error::{AppError, AppResult};

pub const LABEL: &str = "capture";
pub const MAIN_LABEL: &str = "main";

/// Emitted to the panel each time it shows. Mirrored by `Capture`.
const SHOWN: &str = "capture-shown";
/// Emitted to the panel when the shortcut is pressed while it has the keyboard: it plays its
/// exit and hides itself. Mirrored by `Capture`.
const DISMISS: &str = "capture-dismiss";
/// Emitted to the main window with a conversation to open. Mirrored by `App`.
pub const OPEN: &str = "capture-open";

/// The window is wider and taller than the panel it draws, by room for the panel's shadow
/// (`.stage` in Capture.module.css: a 680 px panel); the panel sets the height to fit.
const WIDTH: f64 = 728.0;
const HEIGHT: f64 = 120.0;
/// Where the panel's top sits, as a share of the screen's height (Spotlight is about here).
const TOP: f64 = 0.2;

define_class!(
    // SAFETY: NSPanel adds no requirements to subclassing beyond NSWindow's, and nothing here
    // overrides dealloc.
    #[unsafe(super(NSPanel, NSWindow, NSResponder, NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "CofferCapturePanel"]
    struct CapturePanel;

    impl CapturePanel {
        /// A borderless panel can't take the keyboard by default.
        #[unsafe(method(canBecomeKeyWindow))]
        fn can_become_key_window(&self) -> bool {
            true
        }

        /// Never the main window: that stays Coffer's own.
        #[unsafe(method(canBecomeMainWindow))]
        fn can_become_main_window(&self) -> bool {
            false
        }
    }
);

/// The global-shortcut plugin; every registered shortcut toggles the panel.
pub fn plugin() -> TauriPlugin<Wry> {
    tauri_plugin_global_shortcut::Builder::new()
        .with_handler(|app, _, event| {
            if event.state == ShortcutState::Pressed {
                toggle(app);
            }
        })
        .build()
}

/// Checks a stored or typed shortcut. It needs Command, Control or Option, or it would take a
/// plain key (or Shift with one) away from every app.
pub fn parse(shortcut: &str) -> AppResult<Shortcut> {
    let parsed = Shortcut::from_str(shortcut)
        .map_err(|_| AppError::Invalid("Unsupported shortcut".to_string()))?;
    if !parsed
        .mods
        .intersects(Modifiers::SUPER | Modifiers::CONTROL | Modifiers::ALT)
    {
        return Err(AppError::Invalid(
            "A shortcut needs Command, Control or Option".to_string(),
        ));
    }
    Ok(parsed)
}

/// Makes `shortcut` the one that opens the panel, or none. On failure nothing is registered.
pub fn register(app: &AppHandle, shortcut: Option<&str>) -> AppResult<()> {
    let parsed = shortcut.map(parse).transpose()?;
    let shortcuts = app.global_shortcut();
    shortcuts
        .unregister_all()
        .map_err(|e| AppError::Invalid(e.to_string()))?;
    if let Some(parsed) = parsed {
        shortcuts
            .register(parsed)
            .map_err(|_| AppError::Invalid("Another app uses that shortcut".to_string()))?;
    }
    Ok(())
}

/// Creates the hidden panel. Runs once, on the main thread, during setup.
pub fn create(app: &AppHandle) -> tauri::Result<()> {
    let window = WebviewWindowBuilder::new(app, LABEL, WebviewUrl::App("capture.html".into()))
        .title("Quick Capture")
        .inner_size(WIDTH, HEIGHT)
        .decorations(false)
        .transparent(true)
        .shadow(false)
        .resizable(false)
        .maximizable(false)
        .minimizable(false)
        .skip_taskbar(true)
        .accept_first_mouse(true)
        .visible(false)
        .focused(false)
        .build()?;
    let ns_window = window.ns_window()? as *const NSWindow;
    // SAFETY: the pointer is the window tauri just created and keeps alive; setup runs on the
    // main thread. The window becomes an instance of a subclass of NSPanel, which adds no
    // instance variables to NSWindow's, the way panels are made out of tauri windows.
    unsafe {
        let ns_window = &*ns_window;
        AnyObject::set_class(ns_window, CapturePanel::class());
        ns_window.setStyleMask(ns_window.styleMask() | NSWindowStyleMask::NonactivatingPanel);
        ns_window.setLevel(NSFloatingWindowLevel);
        ns_window.setHidesOnDeactivate(false);
        ns_window.setCollectionBehavior(
            NSWindowCollectionBehavior::CanJoinAllSpaces
                | NSWindowCollectionBehavior::FullScreenAuxiliary
                | NSWindowCollectionBehavior::Transient
                | NSWindowCollectionBehavior::IgnoresCycle,
        );
        let panel = &*(ns_window as *const NSWindow as *const NSPanel);
        panel.setFloatingPanel(true);
        panel.setBecomesKeyOnlyIfNeeded(false);
    }
    Ok(())
}

/// The shortcut: shows the panel, or sends it away if it already has the keyboard.
fn toggle(app: &AppHandle) {
    let Some(window) = app.get_webview_window(LABEL) else {
        return;
    };
    if window.is_visible().unwrap_or(false) && window.is_focused().unwrap_or(false) {
        let _ = window.emit_to(LABEL, DISMISS, ());
    } else {
        show(app, &window);
    }
}

/// Shows the panel on the screen with the pointer and gives it the keyboard, without
/// activating Coffer.
pub fn show(app: &AppHandle, window: &WebviewWindow) {
    if !window.is_visible().unwrap_or(false) {
        place(app, window);
    }
    let panel = window.clone();
    let _ = window.run_on_main_thread(move || {
        let Ok(ns_window) = panel.ns_window() else {
            return;
        };
        // SAFETY: on the main thread, with a window tauri keeps alive.
        unsafe {
            let ns_window = &*(ns_window as *const NSWindow);
            ns_window.orderFrontRegardless();
            ns_window.makeKeyWindow();
        }
    });
    let _ = window.emit_to(LABEL, SHOWN, ());
}

/// Centres the panel near the top of the screen the pointer is on.
fn place(app: &AppHandle, window: &WebviewWindow) {
    let monitor = app
        .cursor_position()
        .ok()
        .and_then(|p| app.monitor_from_point(p.x, p.y).ok().flatten())
        .or_else(|| app.primary_monitor().ok().flatten());
    let (Some(monitor), Ok(size)) = (monitor, window.outer_size()) else {
        return;
    };
    let area = monitor.work_area();
    let x = area.position.x + (area.size.width as i32 - size.width as i32) / 2;
    let y = area.position.y + (area.size.height as f64 * TOP) as i32;
    let _ = window.set_position(PhysicalPosition::new(x, y));
}

/// Brings the main window forward (activating Coffer) and tells it which conversation to open.
pub fn open_in_main(app: &AppHandle, conversation_id: i64) -> AppResult<()> {
    if let Some(panel) = app.get_webview_window(LABEL) {
        let _ = panel.hide();
    }
    let main = app
        .get_webview_window(MAIN_LABEL)
        .ok_or(AppError::NotFound)?;
    let _ = main.unminimize();
    let _ = main.show();
    let _ = main.set_focus();
    let _ = main.emit_to(MAIN_LABEL, OPEN, conversation_id);
    Ok(())
}

/// Fits the window to the panel's height. AppKit keeps the bottom edge of a resized window,
/// so the top is put back: the panel grows and shrinks downwards.
pub fn fit(window: &WebviewWindow, height: f64) {
    let height = height.clamp(HEIGHT / 2.0, 900.0);
    let top = window.outer_position();
    let _ = window.set_size(LogicalSize::new(WIDTH, height));
    if let Ok(top) = top {
        let _ = window.set_position(top);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::settings::DEFAULT_CAPTURE_SHORTCUT;

    #[test]
    fn parses_shortcuts_with_a_real_modifier() {
        assert!(parse(DEFAULT_CAPTURE_SHORTCUT).is_ok());
        assert!(parse("Control+Alt+KeyN").is_ok());
        assert!(parse("Super+Digit1").is_ok());
        // Every key `lib/shortcut.ts` lets the user pick.
        for key in [
            "Space",
            "Enter",
            "Tab",
            "Backspace",
            "ArrowUp",
            "F13",
            "F20",
            "Minus",
            "Equal",
            "BracketLeft",
            "Backslash",
            "Semicolon",
            "Quote",
            "Backquote",
            "Comma",
            "Period",
            "Slash",
        ] {
            assert!(parse(&format!("Alt+{key}")).is_ok(), "{key}");
        }
    }

    #[test]
    fn refuses_plain_keys_and_nonsense() {
        assert!(parse("KeyA").is_err());
        assert!(parse("Shift+Space").is_err());
        assert!(parse("Alt+Shift").is_err());
        assert!(parse("Alt+Nope").is_err());
    }
}
