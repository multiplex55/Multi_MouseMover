mod action;
mod action_handler;
mod keyboard;
mod overlay;

use action::*;
use action_handler::*;
use keyboard::*;
use lazy_static::lazy_static;
use overlay::OVERLAY;
use serde::Deserialize;
use std::collections::HashSet;
use std::sync::RwLock;
use std::thread::sleep;
use std::time::Duration;
use std::{env, fs};
use windows::Win32::Foundation::*;
use windows::Win32::System::LibraryLoader::*;
use windows::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, GetKeyState};
use windows::Win32::UI::WindowsAndMessaging::*;

static mut HOOK_HANDLE: Option<HHOOK> = None;

lazy_static! {
    static ref ACTION_HANDLER: RwLock<ActionHandler> = {
        let config = Config::load_from_file("config.toml");
        let mouse_master = MouseMaster::new(config.clone());
        let handler = ActionHandler::new(mouse_master);

        RwLock::new(handler)
    };
    static ref KEY_ACTIONS: RwLock<KeyBindings> = RwLock::new(KeyBindings::new());
    static ref ACTIVE_KEYS: RwLock<HashSet<VirtualKey>> = RwLock::new(HashSet::new());
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

#[derive(Debug, Deserialize, Clone)]
struct GridSize {
    width: u32,
    height: u32,
}

impl Config {
    fn load_from_file(path: &str) -> Self {
        let config_str = fs::read_to_string(path).expect("Failed to read config file");
        toml::from_str(&config_str).expect("Failed to parse config file")
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

fn main() {
    println!("🚀 Program Start!");

    // Ensure Rust backtrace is enabled
    env::set_var("RUST_BACKTRACE", "1");
    println!("🔹 Backtrace Enabled");

    let config = Config::load_from_file("config.toml");
    println!("✅ Config Loaded");

    config.initialize_bindings();
    println!("✅ Key Bindings Initialized");

    unsafe {
        println!("🔹 Attempting to Get Module Handle...");
        let h_instance = GetModuleHandleW(None).expect("❌ Failed to get module handle!");
        println!("✅ Module Handle Retrieved");

        println!("🔹 Setting Up Keyboard Hook...");
        HOOK_HANDLE = SetWindowsHookExW(
            WH_KEYBOARD_LL,
            Some(keyboard_hook),
            Some(h_instance.into()),
            0,
        )
        .ok();

        if HOOK_HANDLE.is_none() {
            panic!("❌ Keyboard Hook Failed to Install!");
        }
        println!("✅ Keyboard Hook Installed Successfully!");
    }

    println!("🔹 Attempting Overlay Initialization...");
    if let Ok(mut overlay) = OVERLAY.lock() {
        println!("✅ Overlay Initialized Successfully");
        overlay.repaint();
    } else {
        println!("❌ Overlay Lock Failed!");
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
                OVERLAY
                    .lock()
                    .unwrap()
                    .update_overlay_status(is_left_click_held);

                sleep(Duration::from_millis(config.polling_rate));

                // sleep(Duration::from_millis(config.polling_rate));
            }
        }
    }
}
