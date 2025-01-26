mod keyboard;

use keyboard::{KeyBindings, VirtualKey};
use serde::Deserialize;
use std::fs;
use windows::Win32::Foundation::*;
use windows::Win32::System::LibraryLoader::*;
use windows::Win32::UI::Input::KeyboardAndMouse::*;
use windows::Win32::UI::WindowsAndMessaging::*;

static mut HOOK_HANDLE: Option<HHOOK> = None;

#[derive(Debug, Deserialize)]
struct Config {
    key_bindings: Vec<(String, String)>,
}

impl Config {
    fn load_from_file(path: &str) -> Self {
        let config_str = fs::read_to_string(path).expect("Failed to read config file");
        toml::from_str(&config_str).expect("Failed to parse config file")
    }
}

unsafe extern "system" fn keyboard_hook(code: i32, w_param: WPARAM, l_param: LPARAM) -> LRESULT {
    if code == HC_ACTION.try_into().unwrap() {
        let kbd = *(l_param.0 as *const KBDLLHOOKSTRUCT);
        if let Some(virtual_key) = VirtualKey::from_vk_code(kbd.vkCode) {
            println!("Key pressed: {:?}", virtual_key);
        }
    }
    CallNextHookEx(None, code, w_param, l_param)
}

fn main() {
    // Load configuration
    let config = Config::load_from_file("config.toml");

    // Initialize key bindings
    let mut key_bindings = KeyBindings::new();
    for (key, action) in config.key_bindings {
        if let Some(virtual_key) = VirtualKey::from_string(&key) {
            key_bindings.add_binding(virtual_key, action);
        }
    }

    println!("Key bindings loaded: {:?}", key_bindings);

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

    let mut msg = MSG::default();
    unsafe { while GetMessageW(&mut msg, None, 0, 0).as_bool() {} }

    unsafe {
        if let Some(hook) = HOOK_HANDLE {
            UnhookWindowsHookEx(hook).expect("Failed to unhook");
        }
    }
}
