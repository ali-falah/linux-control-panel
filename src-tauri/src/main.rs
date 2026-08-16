// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    // Configure WebKitGTK to use Document Viewer cache model (reduces RAM usage by 50-70%)
    std::env::set_var("WEBKIT_CACHE_MODEL", "DOCUMENT_VIEWER");
    // Optimize memory allocator behavior on Linux
    std::env::set_var("G_SLICE", "always-malloc");

    linux_control_panel_lib::run()
}

