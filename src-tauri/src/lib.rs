use std::fs;
use std::path::PathBuf;

pub mod commands;

use commands::{
    copr_browser::{disable_copr, enable_copr, search_copr},
    cron_manager::{add_cron_job, delete_cron_job, list_cron_jobs},
    dnf_history::{
        dnf_autoremove, dnf_check, dnf_clean_all, dnf_list_versions, dnf_makecache_cmd,
        dnf_package_info, dnf_search_packages, list_dnf_history, undo_transaction,
    },
    env_manager::{read_env_vars, write_env_vars},
    firewall_manager::{
        get_firewall_state, get_zone_rules, modify_firewall_rule, toggle_panic_mode,
    },
    flatpak_rpm::{detect_duplicates, list_flatpaks, list_rpms, remove_flatpak, remove_rpm},
    grub_manager::{read_grub_config, rebuild_grub, write_grub_config},
    hosts_manager::{read_hosts, write_hosts},
    repo_manager::{add_repo, list_repos, run_makecache, toggle_repo},
    selinux_manager::{get_selinux_denials, get_selinux_status, set_selinux_state},
    service_manager::{
        get_service_logs, list_all_units, read_unit_file, unit_action, write_unit_file,
    },
    startup_manager::{
        list_autostart_entries, list_systemd_units, toggle_autostart, toggle_service_unit,
    },
    user_manager::{
        add_group, add_user, change_password, delete_group, delete_user, list_groups, list_users,
        modify_user_group, toggle_sudo,
    },
};

/// Returns the app config directory, creating it if needed.
pub fn config_dir() -> PathBuf {
    let base = dirs::config_dir().unwrap_or_else(|| PathBuf::from("~/.config"));
    let dir = base.join("control-panel");
    let _ = fs::create_dir_all(&dir);
    let _ = fs::create_dir_all(dir.join("logs"));
    dir
}

/// Appends a log entry to the app log file.
pub fn log_to_file(level: &str, message: &str) {
    let dir = config_dir();
    let log_file = dir.join("logs").join("control-panel.log");
    let timestamp = chrono::Local::now().format("%Y-%m-%d %H:%M:%S");
    let entry = format!("[{timestamp}] [{level}] {message}\n");
    let _ = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_file)
        .and_then(|mut f| {
            use std::io::Write;
            f.write_all(entry.as_bytes())
        });
}

/// Check if a binary exists in PATH.
pub async fn binary_exists(name: &str) -> bool {
    tokio::process::Command::new("which")
        .arg(name)
        .output()
        .await
        .map(|o| o.status.success())
        .unwrap_or(false)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    env_logger::init();

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_shell::init())
        .invoke_handler(tauri::generate_handler![
            // Repo Manager
            list_repos,
            toggle_repo,
            add_repo,
            run_makecache,
            // DNF History
            list_dnf_history,
            undo_transaction,
            dnf_search_packages,
            dnf_package_info,
            dnf_list_versions,
            dnf_clean_all,
            dnf_autoremove,
            dnf_check,
            dnf_makecache_cmd,
            // Copr Browser
            search_copr,
            enable_copr,
            disable_copr,
            // Flatpak vs RPM
            list_flatpaks,
            list_rpms,
            detect_duplicates,
            remove_flatpak,
            remove_rpm,
            // Startup Manager
            list_systemd_units,
            list_autostart_entries,
            toggle_service_unit,
            toggle_autostart,
            // Service Manager
            list_all_units,
            unit_action,
            get_service_logs,
            read_unit_file,
            write_unit_file,
            // Hosts Manager
            read_hosts,
            write_hosts,
            // User Manager
            list_users,
            add_user,
            delete_user,
            change_password,
            toggle_sudo,
            list_groups,
            add_group,
            delete_group,
            modify_user_group,
            // Firewall Manager
            get_firewall_state,
            get_zone_rules,
            modify_firewall_rule,
            toggle_panic_mode,
            // GRUB Configurator
            read_grub_config,
            write_grub_config,
            rebuild_grub,
            // SELinux Manager
            get_selinux_status,
            set_selinux_state,
            get_selinux_denials,
            // Cron Manager
            list_cron_jobs,
            add_cron_job,
            delete_cron_job,
            // Environment Manager
            read_env_vars,
            write_env_vars,
        ])
        .plugin(tauri_plugin_dialog::init())
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
