use std::fs;
use std::path::PathBuf;
pub mod utils;
pub mod commands;

use commands::{
    copr_browser::{disable_copr, enable_copr, search_copr},
    cron_manager::{
        list_cron_jobs, add_cron_job, delete_cron_job,
        cron_list_timers, cron_toggle_timer,
    },
    dnf_history::{
        dnf_autoremove, dnf_check, dnf_clean_all, dnf_list_versions, dnf_makecache_cmd,
        dnf_package_info, dnf_search_packages, list_dnf_history, undo_transaction,
        dnf_check_updates, dnf_run_upgrade, dnf_read_log,
        dnf_check_lock_status, dnf_kill_lock, dnf_cancel_upgrade,
    },
    env_manager::{read_env_vars, write_env_vars},
    firewall_manager::{
        get_firewall_state, get_zone_rules, modify_firewall_rule, toggle_panic_mode,
        firewall_get_rich_rules, firewall_get_zone_interfaces, firewall_get_all_interfaces,
        firewall_modify_rich_rule, firewall_change_interface_zone,
    },
    flatpak_rpm::{detect_duplicates, list_flatpaks, list_rpms, remove_flatpak, remove_rpm},
    grub_manager::{read_grub_config, rebuild_grub, write_grub_config},
    hosts_manager::{read_hosts, write_hosts},
    repo_manager::{add_repo, list_repos, run_makecache, toggle_repo, save_repo_details, test_repo_mirror_speeds},
    selinux_manager::{
        get_selinux_denials, get_selinux_status, set_selinux_state,
        selinux_get_booleans, selinux_set_boolean, selinux_explain_denial, selinux_apply_policy_override,
    },
    service_manager::{
        get_service_logs, list_all_units, read_unit_file, unit_action, write_unit_file, get_boot_blame,
    },
    startup_manager::{
        list_autostart_entries, list_systemd_units, toggle_autostart, toggle_service_unit,
    },
    user_manager::{
        add_group, add_user, change_password, delete_group, delete_user, list_groups, list_users,
        modify_user_group, toggle_sudo, user_get_active_sessions, user_kill_session, user_get_ssh_keys, user_save_ssh_keys,
    },
    nginx_manager::{
        nginx_check_installed, nginx_service_status, nginx_service_action, nginx_test_config,
        nginx_get_stats, nginx_list_sites, nginx_toggle_site, nginx_create_site, nginx_delete_site,
        nginx_list_configs, nginx_read_config, nginx_write_config, nginx_list_backups,
        nginx_restore_backup, nginx_list_www, nginx_read_www_file, nginx_create_www_dir,
        nginx_delete_www_entry, nginx_rename_www_entry, nginx_upload_www_file,
        nginx_read_log, nginx_clear_log, nginx_list_log_files,
        nginx_check_certbot, nginx_list_ssl_certs, nginx_renew_cert,
        nginx_generate_reverse_proxy, nginx_get_log_analytics,
    },
    shell_env::{
        shell_list_profile_files, shell_read_profile_file, shell_parse_all_exports,
        shell_get_live_value, shell_write_var, shell_delete_var,
        shell_write_profile_file, shell_create_profile_d_file,
        shell_list_backups, shell_restore_backup,
        shell_parse_path, shell_add_path_entry, shell_remove_path_entry,
        shell_get_live_env, shell_source_file,
    },
    network_manager::{
        network_get_interfaces, network_get_dns,
        network_list_connections, network_get_connection,
        network_save_connection, network_delete_connection,
        network_up_connection, network_down_connection,
        network_set_interface_state,
        network_get_vpn_profiles, network_import_vpn_profile, network_create_vpn_profile,
        network_test_ping, network_test_download, network_test_upload,
    },
    device_manager::{device_get_all, device_get_smart_drives, device_get_smart_data, device_get_topology, device_trigger_self_test},
    app_manager::{
        list_desktop_apps, get_app_meta, get_app_details, uninstall_app,
        get_flatpak_permissions, set_flatpak_permission, get_app_dependencies,
        scan_local_appimages, register_appimage
    },
    system_info::{get_network_interfaces, get_system_stats, get_disk_usage, get_process_list, kill_process, get_network_traffic, get_smart_health, get_os_info, get_disk_io_stats, get_active_connections, get_current_user, ping_interface_gateway, get_system_events, get_network_details, ping_gateway, get_cpu_temperature, get_last_system_update, get_failed_services_count, get_storage_distribution, open_folder},
    journal_viewer::{get_journal_logs},
    security_auditor::{
        security_run_audit,
        security_fix_root_ssh, security_fix_ssh_param,
        security_fix_password_policy, security_fix_firewall, security_fix_selinux,
        security_fix_kernel_param, security_fix_auditd, security_fix_time_sync,
        security_fix_tmp_sticky, security_fix_passwd_perms, security_fix_shadow_perms,
        security_fix_usbguard,
    },
    audit_log::{
        get_auth_events, check_auditd_status, setup_auditd_rules,
        get_command_audit_logs, get_runtime_threats
    },
};
use utils::privilege::{set_sudo_password, clear_sudo_password, check_sudo_status};

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
    let out = tokio::time::timeout(
        tokio::time::Duration::from_secs(5),
        crate::utils::privilege::tokio::Command::new("which")
            .arg(name)
            .output()
    ).await;
    
    match out {
        Ok(Ok(o)) => o.status.success(),
        _ => false,
    }
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
            save_repo_details,
            test_repo_mirror_speeds,
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
            dnf_check_updates,
            dnf_run_upgrade,
            dnf_read_log,
            dnf_check_lock_status,
            dnf_kill_lock,
            dnf_cancel_upgrade,
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
            get_boot_blame,
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
            user_get_active_sessions,
            user_kill_session,
            user_get_ssh_keys,
            user_save_ssh_keys,
            // Firewall Manager
            get_firewall_state,
            get_zone_rules,
            modify_firewall_rule,
            toggle_panic_mode,
            firewall_get_rich_rules,
            firewall_get_zone_interfaces,
            firewall_get_all_interfaces,
            firewall_modify_rich_rule,
            firewall_change_interface_zone,
            // GRUB Configurator
            read_grub_config,
            write_grub_config,
            rebuild_grub,
            // SELinux Manager
            get_selinux_status,
            set_selinux_state,
            get_selinux_denials,
            selinux_get_booleans,
            selinux_set_boolean,
            selinux_explain_denial,
            selinux_apply_policy_override,
            // Cron Manager
            list_cron_jobs,
            add_cron_job,
            delete_cron_job,
            cron_list_timers,
            cron_toggle_timer,
            // Environment Manager
            read_env_vars,
            write_env_vars,
            // Nginx Manager
            nginx_check_installed,
            nginx_service_status,
            nginx_service_action,
            nginx_test_config,
            nginx_get_stats,
            nginx_list_sites,
            nginx_toggle_site,
            nginx_create_site,
            nginx_delete_site,
            nginx_list_configs,
            nginx_read_config,
            nginx_write_config,
            nginx_list_backups,
            nginx_restore_backup,
            nginx_list_www,
            nginx_read_www_file,
            nginx_create_www_dir,
            nginx_delete_www_entry,
            nginx_rename_www_entry,
            nginx_upload_www_file,
            nginx_read_log,
            nginx_clear_log,
            nginx_list_log_files,
            nginx_check_certbot,
            nginx_list_ssl_certs,
            nginx_renew_cert,
            nginx_generate_reverse_proxy,
            nginx_get_log_analytics,
            // Shell Environment
            shell_list_profile_files,
            shell_read_profile_file,
            shell_parse_all_exports,
            shell_get_live_value,
            shell_write_var,
            shell_delete_var,
            shell_write_profile_file,
            shell_create_profile_d_file,
            shell_list_backups,
            shell_restore_backup,
            shell_parse_path,
            shell_add_path_entry,
            shell_remove_path_entry,
            shell_get_live_env,
            shell_source_file,
            // Advanced Network
            network_get_interfaces,
            network_get_dns,
            network_list_connections,
            network_get_connection,
            network_save_connection,
            network_delete_connection,
            network_up_connection,
            network_down_connection,
            network_set_interface_state,
            network_get_vpn_profiles,
            network_import_vpn_profile,
            network_create_vpn_profile,
            network_test_ping,
            network_test_download,
            network_test_upload,
            // Device Manager
            device_get_all,
            device_get_smart_drives,
            device_get_smart_data,
            device_get_topology,
            device_trigger_self_test,
            // App Manager
            list_desktop_apps,
            get_app_meta,
            get_app_details,
            uninstall_app,
            get_flatpak_permissions,
            set_flatpak_permission,
            get_app_dependencies,
            scan_local_appimages,
            register_appimage,
            // System Info
            get_network_interfaces,
            get_system_stats,
            get_disk_usage,
            get_process_list,
            kill_process,
            get_network_traffic,
            get_smart_health,
            get_os_info,
            get_disk_io_stats,
            get_active_connections,
            get_current_user,
            get_storage_distribution,
            get_system_events,
            get_network_details,
            ping_gateway,
            get_cpu_temperature,
            get_last_system_update,
            get_failed_services_count,
            open_folder,
            // Privilege Manager
            set_sudo_password,
            clear_sudo_password,
            check_sudo_status,
            // Journal Viewer
            get_journal_logs,
            // Security Auditor
            security_run_audit,
            security_fix_root_ssh,
            security_fix_ssh_param,
            security_fix_password_policy,
            security_fix_firewall,
            security_fix_selinux,
            security_fix_kernel_param,
            security_fix_auditd,
            security_fix_time_sync,
            security_fix_tmp_sticky,
            security_fix_passwd_perms,
            security_fix_shadow_perms,
            security_fix_usbguard,
            // Audit Log & Threat Detection
            get_auth_events,
            check_auditd_status,
            setup_auditd_rules,
            get_command_audit_logs,
            get_runtime_threats,
        ])
        .plugin(tauri_plugin_dialog::init())
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
