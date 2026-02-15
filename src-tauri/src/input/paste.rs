use anyhow::Result;
use arboard::Clipboard;
use enigo::{Direction, Enigo, Key, Keyboard, Settings};
use std::thread;
use std::time::Duration;

/// Checks if the currently focused UI element is a text input field
/// using the macOS Accessibility API.
#[cfg(target_os = "macos")]
fn is_text_field_focused() -> bool {
    use std::ffi::{c_char, c_void, CString};
    use std::ptr;

    type CFTypeRef = *const c_void;
    type CFStringRef = *const c_void;
    type AXUIElementRef = *mut c_void;
    type AXError = i32;

    const K_CF_STRING_ENCODING_UTF8: u32 = 0x08000100;
    const K_AX_ERROR_SUCCESS: AXError = 0;

    extern "C" {
        fn AXUIElementCreateSystemWide() -> AXUIElementRef;
        fn AXUIElementCopyAttributeValue(
            element: AXUIElementRef,
            attribute: CFStringRef,
            value: *mut CFTypeRef,
        ) -> AXError;
        fn CFRelease(cf: CFTypeRef);
        fn CFStringCreateWithCString(
            alloc: CFTypeRef,
            c_str: *const c_char,
            encoding: u32,
        ) -> CFStringRef;
        fn CFStringGetCStringPtr(theString: CFStringRef, encoding: u32) -> *const c_char;
        fn CFStringGetLength(theString: CFStringRef) -> isize;
        fn CFStringGetCString(
            theString: CFStringRef,
            buffer: *mut c_char,
            bufferSize: isize,
            encoding: u32,
        ) -> bool;
    }

    fn make_cfstring(s: &str) -> CFStringRef {
        let c = CString::new(s).unwrap();
        unsafe { CFStringCreateWithCString(ptr::null(), c.as_ptr(), K_CF_STRING_ENCODING_UTF8) }
    }

    fn cfstring_to_string(cf: CFStringRef) -> Option<String> {
        if cf.is_null() {
            return None;
        }
        unsafe {
            let ptr = CFStringGetCStringPtr(cf, K_CF_STRING_ENCODING_UTF8);
            if !ptr.is_null() {
                return Some(
                    std::ffi::CStr::from_ptr(ptr)
                        .to_string_lossy()
                        .into_owned(),
                );
            }
            let len = CFStringGetLength(cf);
            let mut buf = vec![0u8; (len * 4 + 1) as usize];
            if CFStringGetCString(
                cf,
                buf.as_mut_ptr() as _,
                buf.len() as isize,
                K_CF_STRING_ENCODING_UTF8,
            ) {
                Some(
                    std::ffi::CStr::from_ptr(buf.as_ptr() as _)
                        .to_string_lossy()
                        .into_owned(),
                )
            } else {
                None
            }
        }
    }

    unsafe {
        let system_wide = AXUIElementCreateSystemWide();
        if system_wide.is_null() {
            return false;
        }

        let focused_attr = make_cfstring("AXFocusedUIElement");
        let mut focused: CFTypeRef = ptr::null();
        let err =
            AXUIElementCopyAttributeValue(system_wide, focused_attr, &mut focused);
        CFRelease(system_wide as CFTypeRef);
        CFRelease(focused_attr as CFTypeRef);

        if err != K_AX_ERROR_SUCCESS || focused.is_null() {
            return false;
        }

        let role_attr = make_cfstring("AXRole");
        let mut role: CFTypeRef = ptr::null();
        let err =
            AXUIElementCopyAttributeValue(focused as AXUIElementRef, role_attr, &mut role);
        CFRelease(focused);
        CFRelease(role_attr as CFTypeRef);

        if err != K_AX_ERROR_SUCCESS || role.is_null() {
            return false;
        }

        let role_string = cfstring_to_string(role as CFStringRef);
        CFRelease(role);

        matches!(
            role_string.as_deref(),
            Some(
                "AXTextField" | "AXTextArea" | "AXComboBox" | "AXSearchField" | "AXWebArea"
            )
        )
    }
}

#[cfg(not(target_os = "macos"))]
fn is_text_field_focused() -> bool {
    true
}

/// Pastes transcribed text. When smart_paste is true, checks if a text field
/// is focused first — auto-pastes if so, otherwise saves to clipboard.
/// When smart_paste is false, always attempts immediate paste.
pub fn paste_text(text: &str, smart_paste: bool) -> Result<()> {
    let mut clipboard = Clipboard::new()
        .map_err(|e| anyhow::anyhow!("Failed to access clipboard: {}", e))?;

    let should_auto_paste = !smart_paste || is_text_field_focused();

    if should_auto_paste {
        // Text field is focused — auto-paste and restore clipboard
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
    } else {
        // No text field focused — just save to clipboard for manual pasting
        clipboard
            .set_text(text)
            .map_err(|e| anyhow::anyhow!("Failed to set clipboard text: {}", e))?;
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
