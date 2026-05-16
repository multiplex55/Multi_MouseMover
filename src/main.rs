mod action;
mod action_handler;
mod jump_overlay;
mod keyboard;
mod overlay;

use action::*;
use action_handler::*;
use jump_overlay::{hide_jump_overlay, JUMP_OVERLAY};
use keyboard::*;
use lazy_static::lazy_static;
use overlay::OVERLAY;
use serde::Deserialize;
use std::cell::RefCell;
use std::collections::HashSet;
use std::sync::RwLock;
use std::thread::sleep;
use std::time::Duration;
use std::{env, error::Error, fs, io};
use windows::Win32::Foundation::*;
use windows::Win32::System::LibraryLoader::*;
use windows::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, GetKeyState};
use windows::Win32::UI::WindowsAndMessaging::*;

/// RAII guard for the installed keyboard hook.
struct KeyboardHook(HHOOK);

impl Drop for KeyboardHook {
    fn drop(&mut self) {
        unsafe {
            if !UnhookWindowsHookEx(self.0).as_bool() {
                eprintln!("Failed to unhook keyboard");
            }
        }
    }
}

lazy_static! {
    static ref ACTION_HANDLER: RwLock<ActionHandler> = {
        let config = match Config::load_from_file("config.toml") {
            Ok(cfg) => cfg,
            Err(e) => {
                eprintln!("Failed to load configuration: {}", e);
                std::process::exit(1);
            }
        };
        let mouse_master = MouseMaster::new(config.clone());
        let handler = ActionHandler::new(mouse_master);

        RwLock::new(handler)
    };
    static ref KEY_ACTIONS: RwLock<KeyBindings> = RwLock::new(KeyBindings::new());
    static ref ACTIVE_KEYS: RwLock<HashSet<VirtualKey>> = RwLock::new(HashSet::new());
}

thread_local! {
    /// Thread-local keyboard hook guard.
    ///
    /// The low-level hook handle (`HHOOK`) should be installed and unhooked on the
    /// same thread. Keeping the RAII guard in TLS preserves that ownership model
    /// and avoids sharing the Win32 handle wrapper across threads.
    static KEYBOARD_HOOK_HANDLE: RefCell<Option<KeyboardHook>> = const { RefCell::new(None) };
}

#[derive(Debug, Deserialize, Clone)]
struct Config {
    key_bindings: Vec<(String, String)>,
    polling_rate: u64,
    grid_size: GridSize,
    starting_speed: i32,    // Initial speed in pixels
    acceleration: i32,      // Increment value for acceleration
    acceleration_rate: u32, // Polling cycles before applying acceleration
    top_speed: i32,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            key_bindings: Vec::new(),
            polling_rate: 0,
            grid_size: GridSize::default(),
            starting_speed: 1,
            acceleration: 2,
            acceleration_rate: 1,
            top_speed: 6,
        }
    }
}

#[derive(Debug, Deserialize, Clone)]
struct GridSize {
    width: u32,
    height: u32,
}

impl Default for GridSize {
    fn default() -> Self {
        Self {
            width: 10,
            height: 10,
        }
    }
}

impl Config {
    fn load_from_file(path: &str) -> Result<Self, Box<dyn Error>> {
        // Try to read the config from the provided path relative to the current
        // working directory.  If that fails, fall back to looking in the same
        // directory as the executable.  This allows running the binary from any
        // location as long as `config.toml` sits next to it.

        // DEBUG: print current working directory and executable path
        if let Ok(cwd) = env::current_dir() {
            println!("[DEBUG] current_dir: {}", cwd.display());
        } else {
            println!("[DEBUG] current_dir: <failed>");
        }

        if let Ok(exe) = env::current_exe() {
            println!("[DEBUG] current_exe: {}", exe.display());
        } else {
            println!("[DEBUG] current_exe: <failed>");
        }

        // First attempt: path relative to current directory
        println!("[DEBUG] trying path: {}", path);
        match fs::read_to_string(path) {
            Ok(config_str) => return Ok(toml::from_str(&config_str)?),
            Err(e) => {
                if e.kind() != io::ErrorKind::NotFound {
                    return Err(e.into());
                }
            }
        }

        // Second attempt: path relative to the executable location
        if let Ok(mut exe_path) = env::current_exe() {
            exe_path.pop();
            exe_path.push(path);
            println!("[DEBUG] trying exe path: {}", exe_path.display());
            match fs::read_to_string(&exe_path) {
                Ok(config_str) => return Ok(toml::from_str(&config_str)?),
                Err(e) => {
                    if e.kind() != io::ErrorKind::NotFound {
                        return Err(e.into());
                    }
                }
            }
        }

        eprintln!("Config file not found, using defaults");
        Ok(Self::default())
    }
    fn initialize_bindings(&self) {
        let mut key_actions = KEY_ACTIONS.write().unwrap(); // Acquire write lock

        for (key, action_str) in &self.key_bindings {
            if let Some(virtual_key) = VirtualKey::from_string(key) {
                if let Some(action) = Action::from_string(action_str) {
                    println!("✅ Binding key: {:?} -> {:?}", virtual_key, action);
                    key_actions.add_binding(virtual_key, action);
                } else {
                    println!(
                        "❌ Action '{}' does not exist for key '{}'",
                        action_str, key
                    );
                }
            } else {
                println!("❌ Key '{}' is not recognized", key);
            }
        }
    }
}

unsafe extern "system" fn keyboard_hook(code: i32, w_param: WPARAM, l_param: LPARAM) -> LRESULT {
    if code == HC_ACTION.try_into().unwrap()
        && (w_param.0 as u32 == WM_KEYDOWN
            || w_param.0 as u32 == WM_SYSKEYDOWN
            || w_param.0 as u32 == WM_KEYUP
            || w_param.0 as u32 == WM_SYSKEYUP)
    {
        let kbd = *(l_param.0 as *const KBDLLHOOKSTRUCT);
        if let Some(virtual_key) = VirtualKey::from_vk_code(kbd.vkCode) {
            println!(
                "🔹 Key Event Captured: {:?} | w_param: {}",
                virtual_key, w_param.0
            );

            let key_actions = KEY_ACTIONS.read().unwrap();
            let mut action_handler = ACTION_HANDLER.write().unwrap();
            let mut active_keys = ACTIVE_KEYS.write().unwrap();

            let is_keydown = w_param.0 as u32 == WM_KEYDOWN || w_param.0 as u32 == WM_SYSKEYDOWN;

            if action_handler.mouse_master.jump_active {
                if virtual_key == VirtualKey::Escape && is_keydown {
                    hide_jump_overlay();
                    action_handler.mouse_master.jump_active = false;
                    return LRESULT(1);
                }

                if let Some((x, y)) =
                    JUMP_OVERLAY.with(|overlay| overlay.borrow_mut().handle_key(virtual_key))
                {
                    action_handler.mouse_master.move_mouse_to(x, y);
                    action_handler.mouse_master.jump_active = false;
                }
                return LRESULT(1);
            }

            println!(
                "[DEBUG] Processing Key Event | VirtualKey: {:?} | KeyDown: {}",
                virtual_key, is_keydown
            );

            // ✅ **Detect Alt + E Pressed Together**
            if virtual_key == VirtualKey::E && is_keydown {
                let alt_pressed = (kbd.flags & LLKHF_ALTDOWN)
                    != windows::Win32::UI::WindowsAndMessaging::KBDLLHOOKSTRUCT_FLAGS(0);

                if alt_pressed {
                    println!("[DEBUG] Alt + E detected: Switching mode...");
                    action_handler.mouse_master.toggle_mode();
                    return LRESULT(1);
                }
            }

            // ✅ Always allow `Escape` to exit
            if virtual_key == VirtualKey::Escape && is_keydown {
                println!("[DEBUG] Escape pressed: Exiting...");
                action_handler.mouse_master.exit();
                return LRESULT(1);
            }

            // ✅ Ignore keys if in `Idle Mode`
            if action_handler.mouse_master.current_mode == ModeState::Idle {
                println!("[DEBUG] Idle Mode active: Ignoring key event...");
                return CallNextHookEx(None, code, w_param, l_param);
            }

            // ✅ Normal key processing
            if is_keydown {
                active_keys.insert(virtual_key);
            } else {
                active_keys.remove(&virtual_key);
            }

            // ✅ Process active keys
            for key in active_keys.iter() {
                if let Some(action) = key_actions.get_action(*key) {
                    println!("[DEBUG] Executing keybind: {:?} -> {:?}", key, action);
                    action_handler.process_active_keys(*action, true);
                }
            }

            // ✅ Process key release
            if !is_keydown {
                if let Some(action) = key_actions.get_action(virtual_key) {
                    action_handler.process_active_keys(*action, false);
                }
            }

            return LRESULT(1);
        }
    }
    println!("⚠️ Unhandled key event");
    CallNextHookEx(None, code, w_param, l_param)
}

unsafe fn install_keyboard_hook() -> windows::core::Result<()> {
    println!("🔹 Attempting to Get Module Handle...");
    let h_instance = GetModuleHandleW(None)?;
    println!("✅ Module Handle Retrieved");

    println!("🔹 Setting Up Keyboard Hook...");
    let hook = SetWindowsHookExW(
        WH_KEYBOARD_LL,
        Some(keyboard_hook),
        Some(h_instance.into()),
        0,
    )?;

    // Store the hook guard for cleanup on panic
    KEYBOARD_HOOK_HANDLE.with(|slot| *slot.borrow_mut() = Some(KeyboardHook(hook)));

    Ok(())
}

fn main() {
    println!("🚀 Program Start!");

    // Set a panic hook to ensure we clean up resources on unexpected errors
    std::panic::set_hook(Box::new(|info| {
        eprintln!("Application panicked: {}", info);
        // Drop the hook guard so the keyboard is unhooked
        KEYBOARD_HOOK_HANDLE.with(|slot| {
            slot.borrow_mut().take();
        });
        hide_jump_overlay();
        std::process::exit(1);
    }));

    // Ensure Rust backtrace is enabled
    env::set_var("RUST_BACKTRACE", "1");
    println!("🔹 Backtrace Enabled");

    let config = match Config::load_from_file("config.toml") {
        Ok(cfg) => cfg,
        Err(e) => {
            eprintln!("Error loading configuration: {}", e);
            std::process::exit(1);
        }
    };
    println!("✅ Config Loaded");

    config.initialize_bindings();
    println!("✅ Key Bindings Initialized");

    if let Err(e) = unsafe { install_keyboard_hook() } {
        eprintln!("❌ Keyboard Hook Failed to Install: {e}");
        return;
    }
    println!("✅ Keyboard Hook Installed Successfully!");

    println!("🔹 Attempting Overlay Initialization...");
    match OVERLAY.lock() {
        Ok(mut maybe_ov) => {
            if let Some(ref mut ov) = *maybe_ov {
                println!("✅ Overlay Initialized Successfully");
                ov.repaint();
            } else {
                eprintln!("Overlay disabled due to initialization failure");
            }
        }
        Err(e) => {
            eprintln!("❌ Overlay Lock Failed: {e}");
        }
    }

    println!("🔄 Entering Main Event Loop...");

    loop {
        unsafe {
            let mut msg = MSG::default();
            while PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).as_bool() {
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
                // ✅ Update the overlay position inside the loop
                let is_left_click_held =
                    ACTION_HANDLER.read().unwrap().mouse_master.left_click_held;
                if let Ok(mut maybe_ov) = OVERLAY.lock() {
                    if let Some(ref mut ov) = *maybe_ov {
                        ov.update_overlay_status(is_left_click_held);
                    }
                }

                sleep(Duration::from_millis(config.polling_rate));

                // sleep(Duration::from_millis(config.polling_rate));
            }
        }
    }
}
