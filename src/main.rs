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
                    key_actions.add_binding(virtual_key, action);
                } else {
                    println!("Action '{}' does not exist for key '{}'", action_str, key);
                }
            } else {
                println!("Key '{}' is not recognized", key);
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
            let key_actions = KEY_ACTIONS.read().unwrap();
            let mut action_handler = ACTION_HANDLER.write().unwrap();
            let mut active_keys = ACTIVE_KEYS.write().unwrap();

            let is_keydown = w_param.0 as u32 == WM_KEYDOWN || w_param.0 as u32 == WM_SYSKEYDOWN;

            println!(
                "[DEBUG] Key Event | VirtualKey: {:?} | KeyDown: {}",
                virtual_key, is_keydown
            );

            // ✅ **Detect Alt + E Pressed Together**
            if virtual_key == VirtualKey::E && is_keydown {
                let alt_pressed = (kbd.flags & LLKHF_ALTDOWN)
                    != windows::Win32::UI::WindowsAndMessaging::KBDLLHOOKSTRUCT_FLAGS(0); // Properly detect Alt key being held

                if alt_pressed {
                    println!("[DEBUG] Alt + E detected: Switching mode...");
                    action_handler.mouse_master.toggle_mode();
                    return LRESULT(1);
                }
            }

            // Always allow `Escape` to exit
            if virtual_key == VirtualKey::Escape && is_keydown {
                println!("[DEBUG] Escape pressed: Exiting...");
                action_handler.mouse_master.exit();
                return LRESULT(1);
            }

            // Ignore keys if in `Idle Mode`
            if action_handler.mouse_master.current_mode == ModeState::Idle {
                println!("[DEBUG] Idle Mode active: Ignoring key event...");
                return CallNextHookEx(None, code, w_param, l_param);
            }

            // Normal key processing
            if is_keydown {
                active_keys.insert(virtual_key);
            } else {
                active_keys.remove(&virtual_key);
            }

            for key in active_keys.iter() {
                if let Some(action) = key_actions.get_action(*key) {
                    println!("[DEBUG] Processing key: {:?} -> {:?}", key, action);
                    action_handler.process_active_keys(*action, true);
                }
            }

            if !is_keydown {
                if let Some(action) = key_actions.get_action(virtual_key) {
                    action_handler.process_active_keys(*action, false);
                }
            }

            return LRESULT(1);
        }
    }
    CallNextHookEx(None, code, w_param, l_param)
}

fn main() {
    // Backtrace for Debug
    env::set_var("RUST_BACKTRACE", "1");

    // Load configuration
    let config = Config::load_from_file("config.toml");
    config.initialize_bindings();
    println!("Key bindings loaded from config");
    println!(
        "Loaded grid size: {}x{}",
        config.grid_size.width, config.grid_size.height
    );
    unsafe {
        let h_instance = GetModuleHandleW(None).expect("Failed to get module handle");
        HOOK_HANDLE = SetWindowsHookExW(
            WH_KEYBOARD_LL,
            Some(keyboard_hook),
            Some(h_instance.into()),
            0,
        )
        .ok();
    }

    if let Ok(mut overlay) = OVERLAY.lock() {
        overlay.repaint();
    }

    // Polling loop to control rate
    loop {
        unsafe {
            let mut msg = MSG::default();
            while PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).as_bool() {
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }
        sleep(Duration::from_millis(config.polling_rate));
    }
}
