mod action;
mod keyboard;

use action::*;
use keyboard::*;
use lazy_static::lazy_static;
use serde::Deserialize;
use std::fs;
use std::sync::RwLock;
use std::thread::sleep;
use std::time::Duration;
use windows::Win32::Foundation::*;
use windows::Win32::System::LibraryLoader::*;
use windows::Win32::UI::Input::KeyboardAndMouse::*;
use windows::Win32::UI::WindowsAndMessaging::*;

static mut HOOK_HANDLE: Option<HHOOK> = None;

lazy_static! {
    static ref ACTION_HANDLER: ActionHandler = {
        let mut handler = ActionHandler::new();
        // Default actions
        handler.add_action(Action::MoveUp, || println!("Moving up!"));
        handler.add_action(Action::MoveDown, || println!("Moving down!"));
        handler.add_action(Action::MoveLeft, || println!("Moving left!"));
        handler.add_action(Action::MoveRight, || println!("Moving right!"));
        handler
    };
    // static ref KEY_ACTIONS: KeyBindings = {
    //     let mut bindings = KeyBindings::new();
    //     // Bindings will be dynamically added from config.toml
    //     bindings
    // };
       // Use RwLock for interior mutability to allow modifications
    static ref KEY_ACTIONS: RwLock<KeyBindings> = RwLock::new(KeyBindings::new());
}

#[derive(Debug, Deserialize)]
struct Config {
    key_bindings: Vec<(String, String)>,
    polling_rate: u64,
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
        let kbd = *(l_param.0 as *const KBDLLHOOKSTRUCT);

        if let Some(virtual_key) = VirtualKey::from_vk_code(kbd.vkCode) {
            let key_actions = KEY_ACTIONS.read().unwrap(); // Acquire read lock
            if let Some(action_name) = key_actions.get_action(virtual_key) {
                ACTION_HANDLER.execute_action(action_name);
                return LRESULT(1);
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
                TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }
        sleep(Duration::from_millis(config.polling_rate));
    }
}
