// Keep release builds from opening an additional Windows console.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    aigc_studio_lib::run()
}
