use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use windows::Win32::Foundation::{LPARAM, LRESULT, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, SetWindowsHookExW, UnhookWindowsHookEx, HHOOK, KBDLLHOOKSTRUCT,
    WH_KEYBOARD_LL, WM_KEYDOWN, WM_KEYUP, WM_SYSKEYDOWN, WM_SYSKEYUP,
};

// Virtual key codes for left/right modifier distinction
const VK_LMENU: u32 = 0xA4; // Left Alt
const VK_RCONTROL: u32 = 0xA3; // Right Ctrl
const VK_RSHIFT: u32 = 0xA1; // Right Shift

/// Tracks which modifier keys are currently held down.
static LALT_DOWN: AtomicBool = AtomicBool::new(false);
static RCTRL_DOWN: AtomicBool = AtomicBool::new(false);
static RSHIFT_DOWN: AtomicBool = AtomicBool::new(false);

/// Toggle debounce: timestamp (ms) of last toggle event
static LAST_TOGGLE_MS: AtomicU64 = AtomicU64::new(0);
const TOGGLE_DEBOUNCE_MS: u64 = 500;


/// Wrapper to store HHOOK handle in a static Mutex.
struct HookHandle(*mut core::ffi::c_void);
// Safety: HHOOK is a global Windows handle, safe to send between threads.
unsafe impl Send for HookHandle {}

static HOOK_HANDLE: std::sync::Mutex<Option<HookHandle>> = std::sync::Mutex::new(None);

/// Callback sender — set once at init, used by the hook proc.
static CALLBACK: std::sync::OnceLock<Box<dyn Fn(HotkeyEvent) + Send + Sync>> =
    std::sync::OnceLock::new();

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HotkeyEvent {
    HoldStart,
    HoldStop,
    TogglePressed,
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

/// Low-level keyboard hook callback.
/// Runs on the thread that installed the hook (must have a message pump).
unsafe extern "system" fn hook_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code >= 0 {
        let kb = &*(lparam.0 as *const KBDLLHOOKSTRUCT);
        let vk = kb.vkCode;
        let is_down = wparam.0 == WM_KEYDOWN as usize || wparam.0 == WM_SYSKEYDOWN as usize;
        let is_up = wparam.0 == WM_KEYUP as usize || wparam.0 == WM_SYSKEYUP as usize;

        let prev_lalt = LALT_DOWN.load(Ordering::SeqCst);
        let prev_rctrl = RCTRL_DOWN.load(Ordering::SeqCst);
        let prev_rshift = RSHIFT_DOWN.load(Ordering::SeqCst);
        let prev_both = prev_lalt && prev_rctrl;

        match vk {
            VK_LMENU => {
                if is_down {
                    LALT_DOWN.store(true, Ordering::SeqCst);
                } else if is_up {
                    LALT_DOWN.store(false, Ordering::SeqCst);
                }
            }
            VK_RCONTROL => {
                if is_down {
                    RCTRL_DOWN.store(true, Ordering::SeqCst);
                } else if is_up {
                    RCTRL_DOWN.store(false, Ordering::SeqCst);
                }
            }
            VK_RSHIFT => {
                if is_down {
                    RSHIFT_DOWN.store(true, Ordering::SeqCst);
                } else if is_up {
                    RSHIFT_DOWN.store(false, Ordering::SeqCst);
                }
            }
            _ => {}
        }

        let now_lalt = LALT_DOWN.load(Ordering::SeqCst);
        let now_rctrl = RCTRL_DOWN.load(Ordering::SeqCst);
        let now_rshift = RSHIFT_DOWN.load(Ordering::SeqCst);
        let now_both = now_lalt && now_rctrl;

        if let Some(cb) = CALLBACK.get() {
            // Toggle: all three pressed, RShift was NOT previously down (fresh press only)
            if now_lalt && now_rctrl && now_rshift && !prev_rshift && is_down && vk == VK_RSHIFT {
                let now = now_ms();
                let last = LAST_TOGGLE_MS.load(Ordering::SeqCst);
                if now - last > TOGGLE_DEBOUNCE_MS {
                    LAST_TOGGLE_MS.store(now, Ordering::SeqCst);
                    cb(HotkeyEvent::TogglePressed);
                }
            }
            // Hold start: both lalt+rctrl just became true (and rshift is NOT down)
            else if now_both && !prev_both && !now_rshift {
                cb(HotkeyEvent::HoldStart);
            }
            // Hold stop: was holding both, now one released (and rshift not involved)
            else if !now_both && prev_both && !now_rshift && !prev_rshift {
                cb(HotkeyEvent::HoldStop);
            }
        }
    }

    CallNextHookEx(None, code, wparam, lparam)
}

/// Install the low-level keyboard hook. Must be called from a thread with a message loop.
/// The callback will fire on hotkey events.
pub fn install_hook<F>(callback: F) -> Result<(), String>
where
    F: Fn(HotkeyEvent) + Send + Sync + 'static,
{
    CALLBACK
        .set(Box::new(callback))
        .map_err(|_| "Hook callback already set".to_string())?;

    let hook = unsafe { SetWindowsHookExW(WH_KEYBOARD_LL, Some(hook_proc), None, 0) }
        .map_err(|e| format!("Failed to install keyboard hook: {}", e))?;

    *HOOK_HANDLE.lock().unwrap() = Some(HookHandle(hook.0));

    log::info!("Low-level keyboard hook installed");
    log::info!("  Hold-to-talk: Left Alt + Right Ctrl");
    log::info!("  Toggle-to-talk: Left Alt + Right Ctrl + Right Shift");

    Ok(())
}

/// Remove the keyboard hook. Call on shutdown.
pub fn remove_hook() {
    if let Some(HookHandle(ptr)) = HOOK_HANDLE.lock().unwrap().take() {
        unsafe {
            let _ = UnhookWindowsHookEx(HHOOK(ptr));
        }
        log::info!("Keyboard hook removed");
    }
    // Reset key states
    LALT_DOWN.store(false, Ordering::SeqCst);
    RCTRL_DOWN.store(false, Ordering::SeqCst);
    RSHIFT_DOWN.store(false, Ordering::SeqCst);
}
