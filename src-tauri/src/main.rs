// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use rdev::{listen, Event, EventType, Key};
use std::sync::mpsc;
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

#[derive(Debug, Clone, Copy, PartialEq)]
enum KeyEvent {
    LeftPress,
    LeftRelease,
    RightPress,
    RightRelease,
}

fn eval_understrafe(elapsed: Duration, released_time: &mut Option<SystemTime>, app: AppHandle) {
    let time_passed = elapsed.as_micros();
    if time_passed < (200 * 1000) && time_passed > (1600) {
        app.emit_all(
            "strafe",
            Payload {
                strafe_type: "Early".into(),
                duration: time_passed,
            },
        )
        .unwrap();
    } else if time_passed < 1600 {
        app.emit_all(
            "strafe",
            Payload {
                strafe_type: "Perfect".into(),
                duration: 0,
            },
        )
        .unwrap();
    }
    *released_time = None;
}

fn eval_overstrafe(elapsed: Duration, both_pressed_time: &mut Option<SystemTime>, app: AppHandle) {
    let time_passed = elapsed.as_micros();
    if time_passed < (200 * 1000) {
        app.emit_all(
            "strafe",
            Payload {
                strafe_type: "Late".into(),
                duration: time_passed,
            },
        )
        .unwrap();
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
    // AZERTY detection not implemented for non-Windows platforms
    // TODO: Could use system APIs on macOS/Linux if needed
    false
}

fn is_left_key(key: Key, is_azerty: bool) -> bool {
    match key {
        Key::KeyA if !is_azerty => true,
        Key::KeyQ if is_azerty => true,
        Key::LeftArrow => true,
        _ => false,
    }
}

fn is_right_key(key: Key) -> bool {
    matches!(key, Key::KeyD | Key::RightArrow)
}

fn start_key_listener(tx: mpsc::Sender<KeyEvent>, is_azerty: bool) {
    thread::spawn(move || {
        if let Err(e) = listen(move |event: Event| {
            match event.event_type {
                EventType::KeyPress(key) => {
                    if is_left_key(key, is_azerty) {
                        let _ = tx.send(KeyEvent::LeftPress);
                    } else if is_right_key(key) {
                        let _ = tx.send(KeyEvent::RightPress);
                    }
                }
                EventType::KeyRelease(key) => {
                    if is_left_key(key, is_azerty) {
                        let _ = tx.send(KeyEvent::LeftRelease);
                    } else if is_right_key(key) {
                        let _ = tx.send(KeyEvent::RightRelease);
                    }
                }
                _ => {}
            }
        }) {
            eprintln!("Error starting key listener: {:?}", e);
        }
    });
}

fn main() {
    tauri::Builder::default()
        .setup(|app| {
            let handle = app.handle();
            let is_azerty = is_azerty_layout();

            let (tx, rx) = mpsc::channel::<KeyEvent>();
            start_key_listener(tx, is_azerty);

            tauri::async_runtime::spawn(async move {
                let mut left_pressed = false;
                let mut right_pressed = false;
                let mut both_pressed_time: Option<SystemTime> = None;
                let mut right_released_time: Option<SystemTime> = None;
                let mut left_released_time: Option<SystemTime> = None;

                loop {
                    // Non-blocking receive with small timeout to avoid busy-waiting
                    match rx.recv_timeout(Duration::from_millis(1)) {
                        Ok(event) => match event {
                            KeyEvent::RightRelease if right_pressed => {
                                right_pressed = false;
                                let _ = handle.emit_all("d-released", ());
                                right_released_time = Some(SystemTime::now());
                            }
                            KeyEvent::LeftRelease if left_pressed => {
                                left_pressed = false;
                                let _ = handle.emit_all("a-released", ());
                                left_released_time = Some(SystemTime::now());
                            }
                            KeyEvent::LeftPress if !left_pressed => {
                                left_pressed = true;
                                let _ = handle.emit_all("a-pressed", ());
                                if let Some(x) = right_released_time {
                                    if let Ok(elapsed) = x.elapsed() {
                                        eval_understrafe(
                                            elapsed,
                                            &mut right_released_time,
                                            handle.clone(),
                                        );
                                    }
                                }
                            }
                            KeyEvent::RightPress if !right_pressed => {
                                right_pressed = true;
                                let _ = handle.emit_all("d-pressed", ());
                                if let Some(x) = left_released_time {
                                    if let Ok(elapsed) = x.elapsed() {
                                        eval_understrafe(
                                            elapsed,
                                            &mut left_released_time,
                                            handle.clone(),
                                        );
                                    }
                                }
                            }
                            _ => {}
                        },
                        Err(mpsc::RecvTimeoutError::Timeout) => {
                            // No event, continue checking state
                        }
                        Err(mpsc::RecvTimeoutError::Disconnected) => {
                            eprintln!("Key listener disconnected");
                            break;
                        }
                    }

                    // Evaluation for overstrafe
                    if left_pressed && right_pressed && both_pressed_time.is_none() {
                        both_pressed_time = Some(SystemTime::now());
                    }

                    if (!left_pressed || !right_pressed) && both_pressed_time.is_some() {
                        if let Some(x) = both_pressed_time {
                            if let Ok(elapsed) = x.elapsed() {
                                eval_overstrafe(elapsed, &mut both_pressed_time, handle.clone());
                            }
                        }
                    }
                }
            });
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("failed to run app");
}
