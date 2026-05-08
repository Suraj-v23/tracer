// Prevents additional console window on Windows in release, not relevant on macOS.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    tracer_lib::run()
}
