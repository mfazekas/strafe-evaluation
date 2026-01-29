// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use device_query::{DeviceQuery, DeviceState, Keycode};
use std::thread;
use std::time::{Duration, SystemTime};
use tauri::AppHandle;
use tauri::Manager;

#[cfg(target_os = "windows")]
use winapi::um::winuser::GetKeyboardLayout;

#[derive(Clone, serde::Serialize)]
struct Payload {
    strafe_type: String,
    duration: u128,
}

fn eval_understrafe(elapsed: Duration, released_time: &mut Option<SystemTime>, app: &AppHandle) {
    let time_passed = elapsed.as_micros();
    if time_passed < (200 * 1000) && time_passed > (1600) {
        let _ = app.emit_all(
            "strafe",
            Payload {
                strafe_type: "Early".into(),
                duration: time_passed,
            },
        );
    } else if time_passed < 1600 {
        let _ = app.emit_all(
            "strafe",
            Payload {
                strafe_type: "Perfect".into(),
                duration: 0,
            },
        );
    }
    *released_time = None;
}

fn eval_overstrafe(elapsed: Duration, both_pressed_time: &mut Option<SystemTime>, app: &AppHandle) {
    let time_passed = elapsed.as_micros();
    if time_passed < (200 * 1000) {
        let _ = app.emit_all(
            "strafe",
            Payload {
                strafe_type: "Late".into(),
                duration: time_passed,
            },
        );
    }
    *both_pressed_time = None;
}

#[cfg(target_os = "windows")]
fn is_azerty_layout() -> bool {
    unsafe {
        let layout = GetKeyboardLayout(0);
        let layout_id = layout as u32 & 0xFFFF;
        matches!(layout_id, 0x040C | 0x080C | 0x140C | 0x180C)
    }
}

#[cfg(not(target_os = "windows"))]
fn is_azerty_layout() -> bool {
    false
}

fn is_left_key_pressed(keys: &[Keycode], is_azerty: bool) -> bool {
    let main_key = if is_azerty {
        keys.contains(&Keycode::Q)
    } else {
        keys.contains(&Keycode::A)
    };
    main_key || keys.contains(&Keycode::Left)
}

fn is_right_key_pressed(keys: &[Keycode]) -> bool {
    keys.contains(&Keycode::D) || keys.contains(&Keycode::Right)
}

fn main() {
    tauri::Builder::default()
        .setup(|app| {
            let handle = app.handle();
            let is_azerty = is_azerty_layout();

            thread::spawn(move || {
                let device_state = DeviceState::new();
                let mut left_pressed = false;
                let mut right_pressed = false;
                let mut both_pressed_time: Option<SystemTime> = None;
                let mut right_released_time: Option<SystemTime> = None;
                let mut left_released_time: Option<SystemTime> = None;

                println!("Key listener started (polling mode)");

                loop {
                    let keys: Vec<Keycode> = device_state.get_keys();

                    let left_now = is_left_key_pressed(&keys, is_azerty);
                    let right_now = is_right_key_pressed(&keys);

                    // Left key state change
                    if left_now && !left_pressed {
                        left_pressed = true;
                        let _ = handle.emit_all("a-pressed", ());
                        if let Some(x) = right_released_time {
                            if let Ok(elapsed) = x.elapsed() {
                                eval_understrafe(elapsed, &mut right_released_time, &handle);
                            }
                        }
                    } else if !left_now && left_pressed {
                        left_pressed = false;
                        let _ = handle.emit_all("a-released", ());
                        left_released_time = Some(SystemTime::now());
                    }

                    // Right key state change
                    if right_now && !right_pressed {
                        right_pressed = true;
                        let _ = handle.emit_all("d-pressed", ());
                        if let Some(x) = left_released_time {
                            if let Ok(elapsed) = x.elapsed() {
                                eval_understrafe(elapsed, &mut left_released_time, &handle);
                            }
                        }
                    } else if !right_now && right_pressed {
                        right_pressed = false;
                        let _ = handle.emit_all("d-released", ());
                        right_released_time = Some(SystemTime::now());
                    }

                    // Overstrafe detection
                    if left_pressed && right_pressed && both_pressed_time.is_none() {
                        both_pressed_time = Some(SystemTime::now());
                    }

                    if (!left_pressed || !right_pressed) && both_pressed_time.is_some() {
                        if let Some(x) = both_pressed_time {
                            if let Ok(elapsed) = x.elapsed() {
                                eval_overstrafe(elapsed, &mut both_pressed_time, &handle);
                            }
                        }
                    }

                    thread::sleep(Duration::from_micros(500));
                }
            });
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("failed to run app");
}
