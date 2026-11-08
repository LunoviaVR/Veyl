// Release builds use the GUI subsystem (no console window).
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    veyl_lib::run();
}
