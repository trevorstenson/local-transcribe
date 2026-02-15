use anyhow::Result;
use arboard::Clipboard;
use enigo::{Direction, Enigo, Key, Keyboard, Settings};
use std::thread;
use std::time::Duration;

/// Writes text to the clipboard, simulates Cmd+V to paste it,
/// then restores the original clipboard contents.
pub fn paste_text(text: &str) -> Result<()> {
    let mut clipboard = Clipboard::new()
        .map_err(|e| anyhow::anyhow!("Failed to access clipboard: {}", e))?;

    // Save current clipboard text before overwriting
    let previous_text = clipboard.get_text().ok();

    clipboard
        .set_text(text)
        .map_err(|e| anyhow::anyhow!("Failed to set clipboard text: {}", e))?;

    thread::sleep(Duration::from_millis(50));

    let mut enigo = Enigo::new(&Settings::default())
        .map_err(|e| anyhow::anyhow!("Failed to create enigo instance: {}", e))?;

    enigo
        .key(Key::Meta, Direction::Press)
        .map_err(|e| anyhow::anyhow!("Failed to press Meta key: {}", e))?;
    enigo
        .key(Key::Unicode('v'), Direction::Click)
        .map_err(|e| anyhow::anyhow!("Failed to click 'v' key: {}", e))?;
    enigo
        .key(Key::Meta, Direction::Release)
        .map_err(|e| anyhow::anyhow!("Failed to release Meta key: {}", e))?;

    // Wait for the paste to be processed by the target application
    thread::sleep(Duration::from_millis(150));

    // Restore previous clipboard contents
    if let Ok(mut cb) = Clipboard::new() {
        match previous_text {
            Some(ref prev) if !prev.is_empty() => {
                let _ = cb.set_text(prev);
            }
            _ => {
                let _ = cb.clear();
            }
        }
    }

    Ok(())
}

/// Checks whether the app has macOS Accessibility permission.
#[cfg(target_os = "macos")]
pub fn check_accessibility_permission() -> bool {
    extern "C" {
        fn AXIsProcessTrusted() -> bool;
    }
    unsafe { AXIsProcessTrusted() }
}

#[cfg(not(target_os = "macos"))]
pub fn check_accessibility_permission() -> bool {
    true
}
