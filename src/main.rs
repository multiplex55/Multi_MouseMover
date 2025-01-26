mod keyboard;

use keyboard::*;
use lazy_static::lazy_static;
use serde::Deserialize;
use std::fs;
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
        handler.add_action("move_up", || println!("Moving up!"));
        handler.add_action("move_down", || println!("Moving down!"));
        handler.add_action("move_left", || println!("Moving left!"));
        handler.add_action("move_right", || println!("Moving right!"));
        handler
    };
    static ref KEY_ACTIONS: KeyBindings = {
        let mut bindings = KeyBindings::new();
        bindings.add_binding(VirtualKey::W, "move_up".to_string());
        bindings.add_binding(VirtualKey::S, "move_down".to_string());
        bindings.add_binding(VirtualKey::A, "move_left".to_string());
        bindings.add_binding(VirtualKey::D, "move_right".to_string());
        bindings
    };
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
}

unsafe extern "system" fn keyboard_hook(code: i32, w_param: WPARAM, l_param: LPARAM) -> LRESULT {
    // if code == HC_ACTION.try_into().unwrap() {
    //     let kbd = *(l_param.0 as *const KBDLLHOOKSTRUCT);
    //     if let Some(virtual_key) = VirtualKey::from_vk_code(kbd.vkCode) {
    //         println!("Key pressed: {:?}", virtual_key);
    //     }
    //     let key_action = KEY_ACTIONS.get_action(virtual_key);
    //     if let Some(action_name) = key_action {
    //         ACTION_HANDLER.execute_action(action_name);
    //     }
    // }
    // CallNextHookEx(None, code, w_param, l_param)
    if code == HC_ACTION.try_into().unwrap() {
        let kbd = *(l_param.0 as *const KBDLLHOOKSTRUCT);

        if let Some(virtual_key) = VirtualKey::from_vk_code(kbd.vkCode) {
            if let Some(action_name) = KEY_ACTIONS.get_action(virtual_key) {
                ACTION_HANDLER.execute_action(action_name);
            }
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

    // let mut msg = MSG::default();
    // unsafe { while GetMessageW(&mut msg, None, 0, 0).as_bool() {} }

    // unsafe {
    //     if let Some(hook) = HOOK_HANDLE {
    //         UnhookWindowsHookEx(hook).expect("Failed to unhook");
    //     }
    // }
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
