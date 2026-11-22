// Release builds are a plain Windows app with no console window.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    drawnvless::run();
}
