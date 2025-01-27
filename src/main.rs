mod action;
mod action_handler;
mod keyboard;

use action::*;
use action_handler::*;
use keyboard::*;
use lazy_static::lazy_static;
use serde::Deserialize;
use std::collections::HashSet;
use std::fs;
use std::sync::RwLock;
use std::thread::sleep;
use std::time::Duration;
use windows::Win32::Foundation::*;
use windows::Win32::System::LibraryLoader::*;
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
    if code == HC_ACTION.try_into().unwrap() {
        // Check if the key event is a KEYDOWN or KEYUP event
        if w_param.0 as u32 == WM_KEYDOWN
            || w_param.0 as u32 == WM_SYSKEYDOWN
            || w_param.0 as u32 == WM_KEYUP
            || w_param.0 as u32 == WM_SYSKEYUP
        {
            let kbd = *(l_param.0 as *const KBDLLHOOKSTRUCT);
            if let Some(virtual_key) = VirtualKey::from_vk_code(kbd.vkCode) {
                let key_actions = KEY_ACTIONS.read().unwrap(); // Acquire read lock
                let mut action_handler = ACTION_HANDLER.write().unwrap();

                // Map VirtualKey to Action and call process_active_keys
                if let Some(action) = key_actions.get_action(virtual_key) {
                    let is_keydown =
                        w_param.0 as u32 == WM_KEYDOWN || w_param.0 as u32 == WM_SYSKEYDOWN;
                    action_handler.process_active_keys(*action, is_keydown);
                    return LRESULT(1); // Prevent further propagation of the key event
                }
            }
        }
    }
    CallNextHookEx(None, code, w_param, l_param)
}

fn main() {
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
