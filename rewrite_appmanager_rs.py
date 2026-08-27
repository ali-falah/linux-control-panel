import re

with open("src-tauri/src/commands/app_manager.rs", "r") as f:
    content = f.read()

new_uninstall = """#[tauri::command]
pub async fn uninstall_app(app_handle: AppHandle, package_id: String, source: String) -> Result<(), String> {
    let mut cmd = if source == "Flatpak" {
        let mut c = Command::new("pkexec");
        c.args(&["flatpak", "uninstall", "-y", &package_id]);
        c
    } else {
        let mut c = Command::new("pkexec");
        c.args(&["dnf", "remove", "-y", &package_id]);
        c
    };

    cmd.stdout(std::process::Stdio::piped()).stderr(std::process::Stdio::piped());

    let mut child = cmd.spawn().map_err(|e| e.to_string())?;

    let stdout = child.stdout.take().unwrap();
    let stderr = child.stderr.take().unwrap();

    let app_handle_clone = app_handle.clone();
    tokio::spawn(async move {
        let mut reader = BufReader::new(stdout).lines();
        while let Ok(Some(line)) = reader.next_line().await {
            let _ = app_handle_clone.emit("uninstall-log", line);
        }
    });

    let app_handle_clone2 = app_handle.clone();
    tokio::spawn(async move {
        let mut reader = BufReader::new(stderr).lines();
        while let Ok(Some(line)) = reader.next_line().await {
            let _ = app_handle_clone2.emit("uninstall-log", line);
        }
    });

    let status = child.wait().await.map_err(|e| e.to_string())?;
    
    if !status.success() {
        return Err(format!("Uninstallation failed with status: {}", status));
    }

    let _ = app_handle.emit("uninstall-log", "\\nUninstallation completed. Running cleanup...");

    let mut cleanup_cmd = if source == "Flatpak" {
        let mut c = Command::new("pkexec");
        c.args(&["flatpak", "uninstall", "--unused", "-y"]);
        c
    } else {
        let mut c = Command::new("pkexec");
        c.args(&["dnf", "autoremove", "-y"]);
        c
    };

    cleanup_cmd.stdout(std::process::Stdio::piped()).stderr(std::process::Stdio::piped());
    let mut cleanup_child = cleanup_cmd.spawn().map_err(|e| e.to_string())?;

    let cleanup_stdout = cleanup_child.stdout.take().unwrap();
    let cleanup_stderr = cleanup_child.stderr.take().unwrap();

    let app_handle_clone3 = app_handle.clone();
    tokio::spawn(async move {
        let mut reader = BufReader::new(cleanup_stdout).lines();
        while let Ok(Some(line)) = reader.next_line().await {
            let _ = app_handle_clone3.emit("uninstall-log", line);
        }
    });

    let app_handle_clone4 = app_handle.clone();
    tokio::spawn(async move {
        let mut reader = BufReader::new(cleanup_stderr).lines();
        while let Ok(Some(line)) = reader.next_line().await {
            let _ = app_handle_clone4.emit("uninstall-log", line);
        }
    });

    let cleanup_status = cleanup_child.wait().await.map_err(|e| e.to_string())?;
    let _ = app_handle.emit("uninstall-log", format!("\\nCleanup finished with code {}.", cleanup_status.code().unwrap_or(1)));

    Ok(())
}"""

content = re.sub(r"#\[tauri::command\]\npub async fn uninstall_app\([\s\S]*?Err\(format!\(\"Command failed with status: \{\}\", status\)\)\n    \}\n\}", new_uninstall, content)

with open("src-tauri/src/commands/app_manager.rs", "w") as f:
    f.write(content)
