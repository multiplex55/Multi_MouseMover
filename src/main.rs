mod action;
mod action_handler;
mod app_state;
mod jump_grid;
mod jump_overlay;
mod keyboard;
mod overlay;

use action::*;
use action_handler::*;
use app_state::{AppCommand, AppState, KeyEvent};
use jump_overlay::{hide_jump_overlay, show_jump_overlay, JumpKeyResult, JUMP_OVERLAY};
use keyboard::*;
use lazy_static::lazy_static;
use overlay::OVERLAY;
use serde::Deserialize;
use std::cell::RefCell;
use std::sync::RwLock;
use std::thread::sleep;
use std::time::Duration;
use std::{env, error::Error, fs, io};
use windows::Win32::Foundation::*;
use windows::Win32::System::LibraryLoader::*;
use windows::Win32::UI::Input::KeyboardAndMouse::GetAsyncKeyState;
use windows::Win32::UI::WindowsAndMessaging::*;

const DEFAULT_POLLING_RATE_MS: u64 = 8;

/// RAII guard for the installed keyboard hook.
struct KeyboardHook(HHOOK);

// SAFETY: `KeyboardHook` is only accessed through `Mutex<Option<KeyboardHook>>` and
// represents an opaque Win32 hook handle. Transferring ownership of the wrapper
// between threads does not permit concurrent use of the raw handle.
unsafe impl Send for KeyboardHook {}

impl Drop for KeyboardHook {
    fn drop(&mut self) {
        unsafe {
            if let Err(err) = UnhookWindowsHookEx(self.0) {
                eprintln!("[Win32] UnhookWindowsHookEx failed: {:?}", err);
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
    static ref APP_STATE: RwLock<AppState> = RwLock::new(AppState::default());
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
#[serde(default)]
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
            polling_rate: DEFAULT_POLLING_RATE_MS,
            grid_size: GridSize::default(),
            starting_speed: 1,
            acceleration: 2,
            acceleration_rate: 1,
            top_speed: 6,
        }
    }
}

#[derive(Debug, Deserialize, Clone)]
#[serde(default)]
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
    fn normalize(mut self) -> Self {
        if self.polling_rate == 0 {
            self.polling_rate = DEFAULT_POLLING_RATE_MS;
        }
        self
    }

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
            Ok(config_str) => return Ok(toml::from_str::<Self>(&config_str)?.normalize()),
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
                Ok(config_str) => return Ok(toml::from_str::<Self>(&config_str)?.normalize()),
                Err(e) => {
                    if e.kind() != io::ErrorKind::NotFound {
                        return Err(e.into());
                    }
                }
            }
        }

        eprintln!("Config file not found, using defaults");
        Ok(Self::default().normalize())
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

        APP_STATE
            .write()
            .unwrap()
            .set_bound_keys(key_actions.bound_keys());
    }
}

fn modifier_down(vk_code: i32) -> bool {
    unsafe { (GetAsyncKeyState(vk_code) & i16::MIN) != 0 }
}

fn decode_key_event(w_param: WPARAM, kbd: KBDLLHOOKSTRUCT) -> Option<KeyEvent> {
    let key = VirtualKey::from_vk_code(kbd.vkCode)?;
    let is_down = w_param.0 as u32 == WM_KEYDOWN || w_param.0 as u32 == WM_SYSKEYDOWN;
    let alt_down = (kbd.flags & LLKHF_ALTDOWN)
        != windows::Win32::UI::WindowsAndMessaging::KBDLLHOOKSTRUCT_FLAGS(0);

    Some(KeyEvent {
        key,
        is_down,
        alt_down,
        ctrl_down: modifier_down(0x11)
            || key == VirtualKey::Ctrl
            || key == VirtualKey::LeftCtrl
            || key == VirtualKey::RightCtrl,
        shift_down: modifier_down(0x10)
            || key == VirtualKey::Shift
            || key == VirtualKey::LeftShift
            || key == VirtualKey::RightShift,
        win_down: modifier_down(0x5B) || modifier_down(0x5C),
    })
}

unsafe extern "system" fn keyboard_hook(code: i32, w_param: WPARAM, l_param: LPARAM) -> LRESULT {
    if code == HC_ACTION.try_into().unwrap()
        && (w_param.0 as u32 == WM_KEYDOWN
            || w_param.0 as u32 == WM_SYSKEYDOWN
            || w_param.0 as u32 == WM_KEYUP
            || w_param.0 as u32 == WM_SYSKEYUP)
    {
        let kbd = *(l_param.0 as *const KBDLLHOOKSTRUCT);
        if let Some(event) = decode_key_event(w_param, kbd) {
            let swallow = {
                let mut app_state = APP_STATE.write().unwrap();
                let swallow = app_state.should_swallow_key(&event);
                app_state.enqueue_key_event(event);
                swallow
            };

            if swallow {
                return LRESULT(1);
            }
        }
    }
    CallNextHookEx(None, code, w_param, l_param)
}

fn process_queued_key_events() {
    loop {
        let event = {
            let mut app_state = APP_STATE.write().unwrap();
            app_state.pop_key_event()
        };

        let Some(event) = event else {
            break;
        };

        let action = KEY_ACTIONS.read().unwrap().get_action(event.key).copied();

        APP_STATE.write().unwrap().route_key_event(event, action);
    }

    loop {
        let command = APP_STATE.write().unwrap().pop_command();

        let Some(command) = command else {
            break;
        };

        execute_app_command(command);
    }
}

fn execute_app_command(command: AppCommand) {
    match command {
        AppCommand::ToggleActiveMode => {
            let active_mode = {
                let mut action_handler = ACTION_HANDLER.write().unwrap();
                action_handler.mouse_master.toggle_mode();
                action_handler.mouse_master.current_mode == ModeState::Active
            };
            APP_STATE.write().unwrap().set_active_mode(active_mode);
        }
        AppCommand::Exit => {
            ACTION_HANDLER.write().unwrap().mouse_master.exit();
        }
        AppCommand::EnterJumpMode { activation_key } => {
            let config = ACTION_HANDLER.read().unwrap().mouse_master.config.clone();
            show_jump_overlay(&config);
            APP_STATE.write().unwrap().enter_jump_mode(activation_key);
        }
        AppCommand::KeyAction { action, is_down } => {
            let mut action_handler = ACTION_HANDLER.write().unwrap();
            action_handler.process_active_keys(action, is_down);

            if is_down && !ActionHandler::is_movement_action(action) {
                action_handler.execute_action(&action);
            }
        }
        AppCommand::JumpInput(event) => {
            let jump_result = JUMP_OVERLAY
                .with(|overlay| overlay.borrow_mut().handle_key(event.key, event.is_down));

            match jump_result {
                JumpKeyResult::Ignored | JumpKeyResult::Consumed | JumpKeyResult::Invalid => {}
                JumpKeyResult::Cancelled => {
                    hide_jump_overlay();
                    APP_STATE.write().unwrap().exit_jump_mode();
                }
                JumpKeyResult::Completed { x, y } => {
                    ACTION_HANDLER
                        .write()
                        .unwrap()
                        .mouse_master
                        .move_mouse_to(x, y);
                    APP_STATE.write().unwrap().exit_jump_mode();
                }
            }
        }
    }
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
                ov.request_repaint();
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
            }
        }

        process_queued_key_events();

        ACTION_HANDLER.write().unwrap().tick_movement();

        // ✅ Update the overlay position inside the loop
        let is_left_click_held = ACTION_HANDLER.read().unwrap().mouse_master.left_click_held;
        if let Ok(mut maybe_ov) = OVERLAY.lock() {
            if let Some(ref mut ov) = *maybe_ov {
                ov.update_overlay_status(is_left_click_held);
            }
        }

        sleep(Duration::from_millis(config.polling_rate));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_config(toml: &str) -> Config {
        toml::from_str::<Config>(toml).unwrap().normalize()
    }

    #[test]
    fn zero_polling_rate_normalizes_to_default() {
        let config = parse_config("polling_rate = 0");

        assert_eq!(config.polling_rate, DEFAULT_POLLING_RATE_MS);
    }

    #[test]
    fn positive_polling_rate_is_preserved() {
        let config = parse_config("polling_rate = 16");

        assert_eq!(config.polling_rate, 16);
    }

    #[test]
    fn missing_values_fall_back_to_defaults() {
        let config = parse_config("");
        let defaults = Config::default();

        assert_eq!(config.polling_rate, defaults.polling_rate);
        assert_eq!(config.grid_size.width, defaults.grid_size.width);
        assert_eq!(config.grid_size.height, defaults.grid_size.height);
        assert_eq!(config.starting_speed, defaults.starting_speed);
        assert_eq!(config.acceleration, defaults.acceleration);
        assert_eq!(config.acceleration_rate, defaults.acceleration_rate);
        assert_eq!(config.top_speed, defaults.top_speed);
        assert!(config.key_bindings.is_empty());
    }
}
