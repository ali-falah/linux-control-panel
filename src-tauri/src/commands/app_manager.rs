use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use crate::utils::privilege::tokio::Command;
use crate::utils::privilege::{build_privileged_tokio_command, is_auth_failure, RootAuth};

/// Is this a Debian based system? The dpkg/dpkg-query tools only exist there —
/// on Fedora they are missing and every call fails with "No such file or
/// directory".
fn is_debian_based() -> bool {
    crate::utils::distro::detect_distro().pkg_manager == "apt"
}
use tokio::io::{AsyncBufReadExt, BufReader};
use tauri::{AppHandle, Emitter};

#[derive(Serialize, Clone)]
pub struct DesktopApp {
    pub name: String,
    pub exec: String,
    pub source: String,
    pub package_id: Option<String>,
    pub file_path: String,
    /// Only AppImages fill this in during the scan; native and Flatpak sizes
    /// arrive in one batched `get_app_metas` call.
    pub size_bytes: u64,
    /// False for a system-wide desktop entry that no package owns: the panel
    /// cannot delete it without an administrator, so it must not offer to.
    pub removable: bool,
    /// `NoDisplay`/`Hidden` entry: not in the application menu. Hidden by
    /// default in the list, but a package whose every launcher is hidden would
    /// otherwise be impossible to find.
    pub hidden: bool,
    /// The entry's icon as a `data:` URL. Icons are inlined instead of served
    /// through an asset protocol so the scan needs no extra CSP or scope
    /// configuration. `None` when the entry has no icon or it is not a format a
    /// webview can draw; the table then falls back to a category icon.
    pub icon: Option<String>,
    /// AppImage only: a user `.desktop` entry that launches this file. The
    /// list uses it to merge the entry's row into the AppImage's, so one
    /// application does not appear twice.
    pub desktop_path: Option<String>,
    /// AppImage only: version parsed from the file name. Native and Flatpak
    /// versions arrive with the batched metadata call instead.
    pub version: String,
    /// AppImage only: the file's modification time, shown as Installed.
    pub install_date: u64,
}

/// A `.desktop` file, before the package that owns it is known.
struct ParsedEntry {
    path: PathBuf,
    name: String,
    exec: String,
    icon: Option<String>,
    is_flatpak: bool,
    is_waydroid: bool,
    hidden: bool,
}

fn parse_desktop_entry(path: &Path) -> Option<ParsedEntry> {
    let content = fs::read_to_string(path).ok()?;
    let mut name = None;
    let mut exec = None;
    let mut icon = None;
    let mut hidden = false;
    let mut waydroid = false;

    for line in content.lines() {
        let line = line.trim();
        if let Some(value) = line.strip_prefix("Name=") {
            if name.is_none() {
                name = Some(value.trim().to_string());
            }
        } else if let Some(value) = line.strip_prefix("Exec=") {
            if exec.is_none() {
                exec = Some(value.trim().to_string());
            }
        } else if let Some(value) = line.strip_prefix("Icon=") {
            if icon.is_none() {
                icon = Some(value.trim().to_string());
            }
        } else if line.eq_ignore_ascii_case("NoDisplay=true")
            || line.eq_ignore_ascii_case("NoDisplay=1")
            || line.eq_ignore_ascii_case("Hidden=true")
            || line.eq_ignore_ascii_case("Hidden=1")
        {
            hidden = true;
        } else if line.starts_with("X-WayDroid") {
            waydroid = true;
        }
    }

    let exec = exec?;
    let path_text = path.to_string_lossy();
    let is_flatpak = path_text.split('/').any(|part| part == "flatpak");
    // Only the Waydroid marker key, its Exec line or its own directory counts.
    // Matching the word anywhere in the file also caught ordinary apps whose
    // comment merely mentioned Waydroid.
    let is_waydroid = waydroid
        || exec.starts_with("waydroid ")
        || path_text.contains("/waydroid/");

    Some(ParsedEntry {
        path: path.to_path_buf(),
        name: name.filter(|value| !value.is_empty())?,
        exec,
        icon: icon.filter(|value| !value.is_empty()),
        is_flatpak,
        is_waydroid,
        hidden,
    })
}

/// Where desktop icons live, user locations first so a user-supplied icon wins
/// over a packaged one with the same name.
fn icon_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();
    if let Some(home) = dirs::home_dir() {
        roots.push(home.join(".local/share/icons"));
        roots.push(home.join(".local/share/pixmaps"));
        roots.push(home.join(".local/share/flatpak/exports/share/icons"));
        roots.push(home.join(".icons"));
    }
    roots.extend([
        PathBuf::from("/usr/share/icons"),
        PathBuf::from("/usr/local/share/icons"),
        PathBuf::from("/usr/share/pixmaps"),
        PathBuf::from("/usr/local/share/pixmaps"),
        PathBuf::from("/var/lib/flatpak/exports/share/icons"),
        PathBuf::from("/var/lib/snapd/desktop/icons"),
    ]);
    roots
}

/// Size directories, best first. 64px is sharp enough for the table without
/// pulling 512px artwork (often hundreds of kilobytes) into every scan.
const ICON_SIZE_PREFERENCE: &[&str] = &[
    "64x64", "48x48", "128x128", "96x96", "256x256", "32x32", "scalable", "24x24", "512x512",
    "22x22", "16x16",
];

/// Lower is better. `None` means the directory is not a usable size, such as a
/// `symbolic` set whose monochrome icons can disappear against the row.
fn icon_size_priority(dir_name: &str) -> Option<u8> {
    let base = dir_name.split('@').next().unwrap_or(dir_name);
    if base.contains("symbolic") {
        return None;
    }
    ICON_SIZE_PREFERENCE
        .iter()
        .position(|size| *size == base)
        .map(|position| position as u8)
}

fn icon_mime(path: &Path) -> Option<&'static str> {
    match path.extension()?.to_str()?.to_ascii_lowercase().as_str() {
        "png" => Some("image/png"),
        "svg" => Some("image/svg+xml"),
        "jpg" | "jpeg" => Some("image/jpeg"),
        "webp" => Some("image/webp"),
        "gif" => Some("image/gif"),
        _ => None,
    }
}

/// Icon names routinely contain dots (`org.gnome.Nautilus`), so only a real
/// image extension may be removed. `Path::file_stem` would treat the last dot
/// of the name itself as an extension and look up `org.gnome`.
fn icon_lookup_key(name: &str) -> String {
    let lower = name.to_ascii_lowercase();
    for extension in [".png", ".svg", ".jpg", ".jpeg", ".webp", ".gif"] {
        if let Some(stem) = lower.strip_suffix(extension) {
            return stem.to_string();
        }
    }
    lower
}

/// Icons an entry can refer to by name, collected once per scan. Icon themes
/// store the same name under many sizes and themes; walking every one per row
/// would rescan the same trees dozens of times.
struct IconIndex {
    by_name: HashMap<String, (u8, PathBuf)>,
}

impl IconIndex {
    fn build() -> Self {
        let mut index = IconIndex {
            by_name: HashMap::new(),
        };
        for root in icon_roots() {
            index.collect_root(&root);
        }
        index
    }

    fn add_file(&mut self, path: &Path, priority: u8) {
        if icon_mime(path).is_none() {
            return;
        }
        let Some(file_name) = path.file_name().and_then(|value| value.to_str()) else {
            return;
        };
        let key = icon_lookup_key(file_name);
        match self.by_name.get(&key) {
            Some((existing, _)) if *existing <= priority => {}
            _ => {
                self.by_name.insert(key, (priority, path.to_path_buf()));
            }
        }
    }

    fn collect_files(&mut self, dir: &Path, priority: u8) {
        let Ok(entries) = fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() {
                self.add_file(&path, priority);
            }
        }
    }

    fn collect_root(&mut self, root: &Path) {
        let Ok(entries) = fs::read_dir(root) else {
            return;
        };
        for entry in entries.flatten() {
            let theme_path = entry.path();
            if theme_path.is_file() {
                // Flat icon roots such as /usr/share/pixmaps, and the cached
                // AppImage icons next to the user's theme directories.
                self.add_file(&theme_path, 0);
                continue;
            }
            let Ok(theme_entries) = fs::read_dir(&theme_path) else {
                continue;
            };
            for size_entry in theme_entries.flatten() {
                let size_path = size_entry.path();
                let Some(priority) =
                    icon_size_priority(&size_entry.file_name().to_string_lossy())
                else {
                    continue;
                };
                self.collect_files(&size_path, priority);
                self.collect_files(&size_path.join("apps"), priority);
            }
        }
    }

    fn resolve(&self, icon: &str) -> Option<PathBuf> {
        let icon = icon.trim();
        if icon.is_empty() {
            return None;
        }
        if icon.contains('/') {
            let path = PathBuf::from(icon);
            return (path.is_absolute() && path.is_file()).then_some(path);
        }
        let key = icon_lookup_key(icon);
        self.by_name
            .get(&key)
            .map(|(_, path)| path.clone())
            .filter(|path| path.is_file())
    }
}

/// Read an icon and inline it. The cap keeps one oversized piece of artwork
/// from bloating the scan result; such an entry falls back to its category
/// icon instead.
fn icon_data_url(path: &Path) -> Option<String> {
    use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};

    let mime = icon_mime(path)?;
    let size = fs::metadata(path).ok()?.len();
    if size > 512 * 1024 {
        return None;
    }
    let bytes = fs::read(path).ok()?;
    Some(format!("data:{mime};base64,{}", BASE64.encode(bytes)))
}

fn resolve_icon_data(
    icon: Option<&str>,
    index: &IconIndex,
    cache: &mut HashMap<PathBuf, Option<String>>,
) -> Option<String> {
    let path = index.resolve(icon?)?;
    cache
        .entry(path.clone())
        .or_insert_with(|| icon_data_url(&path))
        .clone()
}

fn application_dirs() -> Vec<PathBuf> {
    let mut dirs = vec![
        PathBuf::from("/usr/share/applications"),
        PathBuf::from("/var/lib/flatpak/exports/share/applications"),
    ];
    if let Some(home) = dirs::home_dir() {
        dirs.push(home.join(".local/share/applications"));
        dirs.push(home.join(".local/share/flatpak/exports/share/applications"));
    }
    dirs
}

/// Resolve native package ownership for many desktop files in one `rpm -qf`
/// call. `rpm -qf` answers one line per argument in order, using
/// `file … is not owned by any package` for the misses, so the paths can be
/// zipped with the output. It exits non-zero for misses — that is expected.
async fn rpm_owners(paths: &[PathBuf]) -> HashMap<PathBuf, String> {
    let mut owners = HashMap::new();
    if paths.is_empty() {
        return owners;
    }

    let mut args: Vec<std::ffi::OsString> = vec!["-qf".into()];
    args.extend(paths.iter().map(|path| path.as_os_str().to_os_string()));
    if let Ok(output) = tokio::process::Command::new("rpm").args(&args).output().await {
        let text = String::from_utf8_lossy(&output.stdout).to_string();
        let lines: Vec<&str> = text.lines().map(str::trim).collect();
        if lines.len() == paths.len() {
            for (path, line) in paths.iter().zip(lines) {
                if line.is_empty()
                    || line.starts_with("file ")
                    || line.starts_with("error:")
                    || line.contains("is not owned")
                {
                    continue;
                }
                owners.insert(path.clone(), line.to_string());
            }
            return owners;
        }
    }

    // A desktop file can disappear between the directory scan and this call, in
    // which case the one-line-per-argument assumption no longer holds. Ask per
    // file instead — a rare path, so the extra spawns do not matter.
    for path in paths {
        if let Ok(output) = tokio::process::Command::new("rpm")
            .arg("-qf")
            .arg(path)
            .output()
            .await
        {
            if output.status.success() {
                let package = String::from_utf8_lossy(&output.stdout).trim().to_string();
                if !package.is_empty() {
                    owners.insert(path.clone(), package);
                }
            }
        }
    }
    owners
}

async fn dpkg_owners(paths: &[PathBuf]) -> HashMap<PathBuf, String> {
    let mut owners = HashMap::new();
    if paths.is_empty() {
        return owners;
    }
    let mut command = tokio::process::Command::new("dpkg");
    command.arg("-S");
    command.args(paths);
    let Ok(output) = command.output().await else {
        return owners;
    };
    for line in String::from_utf8_lossy(&output.stdout).lines() {
        let Some((package, files)) = line.split_once(": ") else {
            continue;
        };
        for file in files.split(", ") {
            let path = PathBuf::from(file.trim());
            if paths.contains(&path) {
                owners
                    .entry(path)
                    .or_insert_with(|| package.trim().to_string());
            }
        }
    }
    owners
}

#[tauri::command]
pub async fn list_desktop_apps() -> Result<Vec<DesktopApp>, String> {
    let mut desktop_files: Vec<PathBuf> = Vec::new();
    let mut seen_paths: HashSet<PathBuf> = HashSet::new();
    for dir in application_dirs() {
        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() && path.extension().is_some_and(|ext| ext == "desktop") {
                if seen_paths.insert(path.clone()) {
                    desktop_files.push(path);
                }
            }
        }
    }

    let mut parsed: Vec<ParsedEntry> = desktop_files
        .iter()
        .filter_map(|path| parse_desktop_entry(path))
        .collect();

    // Deduplication keeps the first entry per key. A package can ship a hidden
    // and a visible launcher under the same name; the visible one must win, or
    // the application would vanish while hidden entries are filtered out.
    parsed.sort_by_key(|entry| entry.hidden);

    // One ownership lookup for every native entry, rather than one `rpm -qf`
    // spawn per desktop file.
    let native_paths: Vec<PathBuf> = parsed
        .iter()
        .filter(|entry| !entry.is_flatpak && !entry.is_waydroid)
        .map(|entry| entry.path.clone())
        .collect();
    let mut owners = rpm_owners(&native_paths).await;
    if is_debian_based() {
        let missing: Vec<PathBuf> = native_paths
            .iter()
            .filter(|path| !owners.contains_key(*path))
            .cloned()
            .collect();
        owners.extend(dpkg_owners(&missing).await);
    }

    let mut apps: Vec<DesktopApp> = Vec::new();
    let mut seen_keys: HashSet<(String, String, String)> = HashSet::new();
    let icon_index = IconIndex::build();
    let mut icon_cache: HashMap<PathBuf, Option<String>> = HashMap::new();

    for entry in parsed {
        let file_path = entry.path.to_string_lossy().to_string();
        let stem = entry
            .path
            .file_stem()
            .map(|value| value.to_string_lossy().to_string());

        let (source, package_id) = if entry.is_flatpak {
            ("Flatpak", stem)
        } else if entry.is_waydroid {
            ("Waydroid", stem)
        } else if let Some(package) = owners.get(&entry.path) {
            ("RPM", Some(package.clone()))
        } else {
            ("Local", stem)
        };

        // Collapse launchers that describe the same application (same owner and
        // the same visible name), such as a `.desktop` file exported twice.
        // Different apps from one package (Writer and Calc) keep their own row,
        // and Local entries are keyed by file so a system and a user copy stay
        // distinct.
        let owner = if source == "Local" {
            file_path.clone()
        } else {
            package_id.clone().unwrap_or_else(|| file_path.clone())
        };
        let key = (
            source.to_string(),
            owner,
            entry.name.trim().to_lowercase(),
        );
        if !seen_keys.insert(key) {
            continue;
        }

        // A user-directory entry can still be a symlink into /usr, which the
        // removal path refuses. Resolve it here so the row never offers an
        // action that cannot succeed.
        let removable = if source == "Local" {
            dirs::home_dir()
                .map(|home| {
                    let user_dir = home.join(".local/share/applications");
                    let resolved_dir = user_dir.canonicalize().unwrap_or(user_dir);
                    entry
                        .path
                        .canonicalize()
                        .map(|resolved| resolved.parent() == Some(resolved_dir.as_path()))
                        .unwrap_or(false)
                })
                .unwrap_or(false)
        } else {
            true
        };

        let icon = resolve_icon_data(entry.icon.as_deref(), &icon_index, &mut icon_cache);

        apps.push(DesktopApp {
            name: entry.name,
            exec: entry.exec,
            source: source.to_string(),
            package_id,
            file_path,
            size_bytes: 0,
            removable,
            hidden: entry.hidden,
            icon,
            desktop_path: None,
            version: String::new(),
            install_date: 0,
        });
    }

    apps.sort_by(|a, b| {
        a.name
            .to_lowercase()
            .cmp(&b.name.to_lowercase())
            .then_with(|| a.source.cmp(&b.source))
            .then_with(|| a.file_path.cmp(&b.file_path))
    });

    Ok(apps)
}

#[derive(Serialize, Clone)]
pub struct AppMeta {
    pub size_bytes: u64,
    pub install_date: u64,
    pub version: String,
}

#[derive(Deserialize)]
pub struct AppMetaRequest {
    pub package_id: String,
    pub source: String,
}

async fn flatpak_size(app_id: &str) -> u64 {
    if let Ok(output) = Command::new("flatpak")
        .arg("info")
        .arg("--show-size")
        .arg(app_id)
        .output()
        .await
    {
        if output.status.success() {
            let value = String::from_utf8_lossy(&output.stdout).trim().to_string();
            return value.parse::<u64>().unwrap_or(0);
        }
    }
    0
}

/// When Flatpak last deployed this application.
///
/// `flatpak list` has no install time, and `flatpak history` prints times
/// without a year. The `current/active` link is rewritten on every deploy, so
/// the mtime of the deployment it points at is the install / last-update time.
fn flatpak_install_time(app_id: &str) -> u64 {
    if !crate::commands::flatpak_rpm::is_safe_package_id(app_id) {
        return 0;
    }
    let mut roots = vec![PathBuf::from("/var/lib/flatpak/app")];
    if let Some(home) = dirs::home_dir() {
        roots.push(home.join(".local/share/flatpak/app"));
    }
    for root in roots {
        let active = root.join(app_id).join("current").join("active");
        let Ok(metadata) = fs::metadata(&active) else {
            continue;
        };
        if let Ok(modified) = metadata.modified() {
            if let Ok(since_epoch) = modified.duration_since(std::time::UNIX_EPOCH) {
                return since_epoch.as_secs();
            }
        }
    }
    0
}

async fn rpm_meta_single(package_id: &str) -> AppMeta {
    if let Ok(output) = Command::new("rpm")
        .arg("-q")
        .arg("--queryformat")
        .arg("%{SIZE}|%{INSTALLTIME}")
        .arg(package_id)
        .output()
        .await
    {
        if output.status.success() {
            let value = String::from_utf8_lossy(&output.stdout).trim().to_string();
            let parts: Vec<&str> = value.split('|').collect();
            if parts.len() == 3 {
                let size_bytes = parts[0].parse::<u64>().unwrap_or(0);
                let install_date = parts[1].parse::<u64>().unwrap_or(0);
                let version = parts[2].trim().to_string();
                if size_bytes > 0 {
                    return AppMeta {
                        size_bytes,
                        install_date,
                        version,
                    };
                }
            }
        }
    }
    if is_debian_based() {
        if let Ok(output) = Command::new("dpkg-query")
            .args(["-W", "-f=${Installed-Size}", package_id])
            .output()
            .await
        {
            if output.status.success() {
                let value = String::from_utf8_lossy(&output.stdout).trim().to_string();
                if let Ok(kb) = value.parse::<u64>() {
                    return AppMeta {
                        size_bytes: kb * 1024,
                        install_date: 0,
                        version: String::new(),
                    };
                }
            }
        }
    }
    AppMeta {
        size_bytes: 0,
        install_date: 0,
        version: String::new(),
    }
}

/// Metadata for the whole list in one call: one `rpm -q` for every native
/// package, instead of one spawn per row while the table is still loading.
#[tauri::command]
pub async fn get_app_metas(items: Vec<AppMetaRequest>) -> Vec<AppMeta> {
    let mut result = vec![
        AppMeta {
            size_bytes: 0,
            install_date: 0,
            version: String::new(),
        };
        items.len()
    ];

    let native: Vec<(usize, String)> = items
        .iter()
        .enumerate()
        .filter(|(_, item)| item.source == "RPM" && crate::commands::flatpak_rpm::is_safe_package_id(&item.package_id))
        .map(|(index, item)| (index, item.package_id.clone()))
        .collect();

    let flatpak_versions: HashMap<String, String> = {
        let mut versions = HashMap::new();
        if items.iter().any(|item| item.source == "Flatpak") {
            if let Ok(output) = tokio::process::Command::new("flatpak")
                .args(["list", "--app", "--columns=application,version"])
                .output()
                .await
            {
                for line in String::from_utf8_lossy(&output.stdout).lines() {
                    if let Some((id, version)) = line.split_once('\t') {
                        versions.insert(id.trim().to_string(), version.trim().to_string());
                    }
                }
            }
        }
        versions
    };

    if !native.is_empty() {
        let mut args: Vec<String> = vec![
            "-q".to_string(),
            "--queryformat".to_string(),
            "%{NAME}-%{VERSION}-%{RELEASE}.%{ARCH}|%{SIZE}|%{INSTALLTIME}|%{VERSION}\n".to_string(),
        ];
        args.extend(native.iter().map(|(_, id)| id.clone()));
        if let Ok(output) = tokio::process::Command::new("rpm").args(&args).output().await {
            let text = String::from_utf8_lossy(&output.stdout).to_string();
            let mut by_id: HashMap<String, AppMeta> = HashMap::new();
            for line in text.lines() {
                let parts: Vec<&str> = line.split('|').collect();
                if parts.len() != 4 {
                    continue;
                }
                by_id.insert(
                    parts[0].to_string(),
                    AppMeta {
                        size_bytes: parts[1].parse().unwrap_or(0),
                        install_date: parts[2].parse().unwrap_or(0),
                        version: parts[3].trim().to_string(),
                    },
                );
            }
            for (index, id) in &native {
                if let Some(meta) = by_id.get(id) {
                    result[*index] = meta.clone();
                }
            }
        }
    }

    for (index, item) in items.iter().enumerate() {
        if result[index].size_bytes > 0 || result[index].install_date > 0 {
            continue;
        }
        match item.source.as_str() {
            "Flatpak" => {
                if crate::commands::flatpak_rpm::is_safe_package_id(&item.package_id) {
                    result[index] = AppMeta {
                        size_bytes: flatpak_size(&item.package_id).await,
                        install_date: flatpak_install_time(&item.package_id),
                        version: flatpak_versions
                            .get(&item.package_id)
                            .cloned()
                            .unwrap_or_default(),
                    };
                }
            }
            "RPM" => {
                if crate::commands::flatpak_rpm::is_safe_package_id(&item.package_id) {
                    result[index] = rpm_meta_single(&item.package_id).await;
                }
            }
            _ => {}
        }
    }

    result
}

/// Single-app variant kept for callers that only need one row.
#[tauri::command]
pub async fn get_app_meta(package_id: String, source: String) -> Result<AppMeta, String> {
    let mut metas = get_app_metas(vec![AppMetaRequest { package_id, source }]).await;
    Ok(metas.pop().unwrap_or(AppMeta {
        size_bytes: 0,
        install_date: 0,
        version: String::new(),
    }))
}

#[derive(Serialize)]
pub struct AppDetails {
    pub version: String,
    pub description: String,
    pub files: Vec<String>,
}

#[tauri::command]
pub async fn get_app_details(package_id: String, source: String) -> Result<AppDetails, String> {
    if !["Flatpak", "Waydroid", "Local", "RPM"].contains(&source.as_str()) {
        return Err("Unsupported application source".to_string());
    }
    if source != "Local" && !crate::commands::flatpak_rpm::is_safe_package_id(&package_id) {
        return Err("Invalid application package ID".to_string());
    }
    if source == "Flatpak" {
        let mut version = "Unknown".to_string();
        let description = "No description available.".to_string();
        
        if let Ok(output) = Command::new("flatpak").arg("info").arg(&package_id).output().await {
            if output.status.success() {
                let info = String::from_utf8_lossy(&output.stdout).to_string();
                for line in info.lines() {
                    if line.trim().starts_with("Version:") {
                        version = line.splitn(2, ':').nth(1).unwrap_or("").trim().to_string();
                    }
                }
            }
        }
        
        Ok(AppDetails {
            version,
            description,
            files: Vec::new(),
        })
    } else if source == "Waydroid" {
        let pkg = package_id.trim_start_matches("waydroid.");
        Ok(AppDetails {
            version: "Android App".to_string(),
            description: format!("Waydroid Android package ({})", pkg),
            files: Vec::new(),
        })
    } else if source == "Local" {
        Ok(AppDetails {
            version: String::new(),
            description: "Desktop launcher / user-installed local program.".to_string(),
            files: Vec::new(),
        })
    } else {
        // RPM or DEB
        let mut version = "Unknown".to_string();
        let mut description = String::new();
        let mut files = Vec::new();
        
        if let Ok(output) = Command::new("rpm").arg("-qi").arg(&package_id).output().await {
            if output.status.success() {
                let info = String::from_utf8_lossy(&output.stdout).to_string();
                let mut in_desc = false;
                for line in info.lines() {
                    if in_desc {
                        description.push_str(line);
                        description.push('\n');
                    } else if line.starts_with("Version") {
                        if let Some(idx) = line.find(':') {
                            version = line[idx+1..].trim().to_string();
                        }
                    } else if line.starts_with("Description") {
                        in_desc = true;
                    }
                }
            }
        }

        if version == "Unknown" && is_debian_based() {
            if let Ok(output) = Command::new("dpkg-query").args(["-s", &package_id]).output().await {
                if output.status.success() {
                    let info = String::from_utf8_lossy(&output.stdout).to_string();
                    let mut in_desc = false;
                    for line in info.lines() {
                        if in_desc {
                            if line.starts_with(' ') {
                                description.push_str(line.trim());
                                description.push('\n');
                            } else {
                                in_desc = false;
                            }
                        }
                        if line.starts_with("Version:") {
                            version = line.trim_start_matches("Version:").trim().to_string();
                        } else if line.starts_with("Description:") {
                            description.push_str(line.trim_start_matches("Description:").trim());
                            description.push('\n');
                            in_desc = true;
                        }
                    }
                }
            }
        }
        
        if let Ok(output) = Command::new("rpm").arg("-ql").arg(&package_id).output().await {
            if output.status.success() {
                let list = String::from_utf8_lossy(&output.stdout).to_string();
                for line in list.lines() {
                    files.push(line.to_string());
                }
            }
        }
        if files.is_empty() && is_debian_based() {
            if let Ok(output) = Command::new("dpkg-query").args(["-L", &package_id]).output().await {
                if output.status.success() {
                    let list = String::from_utf8_lossy(&output.stdout).to_string();
                    for line in list.lines() {
                        files.push(line.to_string());
                    }
                }
            }
        }
        
        Ok(AppDetails {
            version,
            description: description.trim().to_string(),
            files,
        })
    }
}

/// Run one removal and stream its output so the panel can show progress.
async fn run_removal(
    program: &str,
    args: &[String],
    privileged: bool,
    app_handle: Option<&AppHandle>,
) -> Result<(bool, String, String), String> {
    use std::process::Stdio;

    let mut command = if privileged {
        let (command, _password) =
            build_privileged_tokio_command(program, args, &RootAuth::Polkit)?;
        command
    } else {
        let mut command = tokio::process::Command::new(program);
        command.args(args);
        command
    };
    command.stdout(Stdio::piped()).stderr(Stdio::piped());

    let mut child = command
        .spawn()
        .map_err(|e| format!("Failed to start {program}: {e}"))?;
    let stdout = child.stdout.take().ok_or("Could not read the command output")?;
    let stderr = child.stderr.take().ok_or("Could not read the command output")?;

    let stdout_task = {
        let handle = app_handle.cloned();
        tokio::spawn(async move {
            let mut reader = BufReader::new(stdout).lines();
            let mut collected = String::new();
            while let Ok(Some(line)) = reader.next_line().await {
                collected.push_str(&line);
                collected.push('\n');
                if let Some(handle) = &handle {
                    let _ = handle.emit("uninstall-log", &line);
                }
            }
            collected
        })
    };
    let stderr_task = {
        let handle = app_handle.cloned();
        tokio::spawn(async move {
            let mut reader = BufReader::new(stderr).lines();
            let mut collected = String::new();
            while let Ok(Some(line)) = reader.next_line().await {
                collected.push_str(&line);
                collected.push('\n');
                if let Some(handle) = &handle {
                    let _ = handle.emit("uninstall-log", &line);
                }
            }
            collected
        })
    };

    let status = child.wait().await.map_err(|e| e.to_string())?;
    // Await the readers too: `wait()` can return while the last buffered lines
    // are still being drained, and losing the error tail hides the reason a
    // removal failed.
    let stdout_text = stdout_task.await.unwrap_or_default();
    let stderr_text = stderr_task.await.unwrap_or_default();

    Ok((status.success(), stdout_text, stderr_text))
}

fn output_tail(stdout: &str, stderr: &str) -> String {
    let source = if stderr.trim().is_empty() { stdout } else { stderr };
    source
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .rev()
        .take(8)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect::<Vec<_>>()
        .join("\n")
}

/// Remove a desktop entry the panel found but no package owns. Only files
/// directly inside the user's `~/.local/share/applications` are ours to delete;
/// a system-wide unowned entry needs an administrator and is refused instead of
/// reporting a success that changed nothing.
fn remove_local_entry(
    package_id: &str,
    file_path: Option<&str>,
    app_handle: Option<&AppHandle>,
) -> Result<String, String> {
    let home = dirs::home_dir().ok_or_else(|| "Could not find home directory".to_string())?;
    let user_apps = home.join(".local/share/applications");
    let candidate = match file_path {
        Some(path) => PathBuf::from(path),
        None => user_apps.join(format!("{package_id}.desktop")),
    };
    let resolved = candidate.canonicalize().map_err(|_| {
        format!("The desktop entry for '{package_id}' no longer exists. Refresh the list.")
    })?;

    let user_apps_resolved = user_apps.canonicalize().unwrap_or(user_apps.clone());
    let is_desktop = resolved.extension().map(|ext| ext == "desktop") == Some(true);
    if resolved.parent() != Some(user_apps_resolved.as_path()) || !is_desktop {
        return Err(
            "This launcher is installed system-wide and is not owned by a package; it can only be removed by an administrator."
                .to_string(),
        );
    }

    fs::remove_file(&resolved)
        .map_err(|e| format!("Could not remove the desktop entry: {e}"))?;
    if resolved.exists() {
        return Err("The desktop entry is still present after removal.".to_string());
    }
    if let Some(handle) = app_handle {
        let _ = handle.emit(
            "uninstall-log",
            format!("Removed {}", resolved.display()),
        );
    }
    Ok(String::new())
}

/// The single removal path for every application source. `uninstall_app` and
/// the Duplicates view's `remove_rpm` / `remove_flatpak` all end up here, so
/// the safety checks, privilege route and logging cannot drift apart.
pub async fn remove_application(
    app_handle: Option<&AppHandle>,
    package_id: &str,
    source: &str,
    system_wide: bool,
    expected_preview: Option<&str>,
    file_path: Option<&str>,
) -> Result<String, String> {
    match source {
        "Local" => return remove_local_entry(package_id, file_path, app_handle),
        "AppImage" => {
            return Err("AppImages are removed with the Delete AppImage action.".to_string())
        }
        "Waydroid" => {
            let android_package = package_id.trim_start_matches("waydroid.");
            if android_package.is_empty()
                || !crate::commands::flatpak_rpm::is_safe_package_id(android_package)
            {
                return Err("Invalid Waydroid application ID".to_string());
            }
            let output = Command::new("waydroid")
                .args(["app", "remove", android_package])
                .output()
                .await
                .map_err(|e| format!("Failed to run waydroid: {e}"))?;
            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr).to_string();
                return Err(format!("Failed to remove the Waydroid app: {}", stderr.trim()));
            }
            return Ok(String::from_utf8_lossy(&output.stdout).to_string());
        }
        "RPM" | "Flatpak" => {}
        _ => return Err("Unsupported application source".to_string()),
    }

    if !crate::commands::flatpak_rpm::is_safe_package_id(package_id) {
        return Err("Invalid application package ID".to_string());
    }
    if source == "RPM" {
        if crate::commands::flatpak_rpm::is_protected_package(package_id) {
            return Err(format!(
                "Action blocked: '{}' is a vital system package and cannot be uninstalled.",
                package_id
            ));
        }
        let expected = expected_preview
            .ok_or_else(|| "Review the package removal preview before continuing".to_string())?;
        let current = crate::commands::flatpak_rpm::rpm_remove_preview(package_id).await?;
        if !crate::commands::flatpak_rpm::same_removal_transaction(&current, expected) {
            return Err("The removal transaction changed. Review the updated preview and confirm again.".to_string());
        }
    }

    let (program, args, privileged): (&str, Vec<String>, bool) = if source == "Flatpak" && system_wide {
        (
            "/usr/bin/flatpak",
            vec![
                "uninstall".to_string(),
                "--system".to_string(),
                "-y".to_string(),
                package_id.to_string(),
            ],
            true,
        )
    } else if source == "Flatpak" {
        (
            "/usr/bin/flatpak",
            vec![
                "uninstall".to_string(),
                "--user".to_string(),
                "-y".to_string(),
                package_id.to_string(),
            ],
            false,
        )
    } else if crate::binary_exists("dnf").await {
        (
            "/usr/bin/dnf",
            vec![
                "remove".to_string(),
                "-y".to_string(),
                package_id.to_string(),
            ],
            true,
        )
    } else if crate::binary_exists("apt-get").await {
        (
            "/usr/bin/apt-get",
            vec![
                "remove".to_string(),
                "-y".to_string(),
                package_id.to_string(),
            ],
            true,
        )
    } else {
        return Err("No supported package manager is available".to_string());
    };

    let (success, stdout, stderr) = run_removal(program, &args, privileged, app_handle).await?;
    if !success {
        if is_auth_failure(&stderr, &stdout) {
            return Err(
                "Administrator authentication was cancelled or failed. Nothing was removed."
                    .to_string(),
            );
        }
        let tail = output_tail(&stdout, &stderr);
        return Err(if tail.is_empty() {
            "The removal command failed without an error message.".to_string()
        } else {
            format!("Removal failed:\n{tail}")
        });
    }

    Ok(stdout)
}

#[tauri::command]
pub async fn uninstall_app(
    app_handle: AppHandle,
    package_id: String,
    source: String,
    expected_preview: Option<String>,
    system_wide: Option<bool>,
    file_path: Option<String>,
) -> Result<(), String> {
    remove_application(
        Some(&app_handle),
        &package_id,
        &source,
        system_wide.unwrap_or(false),
        expected_preview.as_deref(),
        file_path.as_deref(),
    )
    .await?;

    let _ = app_handle.emit(
        "uninstall-log",
        "\nRemoval completed. Automatic dependency cleanup was skipped to protect other installed software.",
    );
    Ok(())
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct FlatpakPermissions {
    pub network: bool,
    pub ipc: bool,
    pub fallback_x11: bool,
    pub x11: bool,
    pub wayland: bool,
    pub pulseaudio: bool,
    pub gpu: bool,
    pub host_files: bool,
    pub home_files: bool,
}

fn parse_flatpak_ini(content: &str, permissions: &mut FlatpakPermissions, _is_override: bool) {
    let mut in_context = false;
    for line in content.lines() {
        let line = line.trim();
        if line.starts_with('[') && line.ends_with(']') {
            in_context = line.to_lowercase() == "[context]";
        } else if in_context {
            if let Some((key, val)) = line.split_once('=') {
                let val = val.trim();
                let items: Vec<&str> = val.split(';').map(|s| s.trim()).filter(|s| !s.is_empty()).collect();
                
                match key.trim().to_lowercase().as_str() {
                    "shared" => {
                        for item in items {
                            let (negated, name) = if item.starts_with('!') { (true, &item[1..]) } else { (false, item) };
                            if name == "network" {
                                permissions.network = !negated;
                            } else if name == "ipc" {
                                permissions.ipc = !negated;
                            }
                        }
                    }
                    "sockets" => {
                        for item in items {
                            let (negated, name) = if item.starts_with('!') { (true, &item[1..]) } else { (false, item) };
                            match name {
                                "x11" => permissions.x11 = !negated,
                                "wayland" => permissions.wayland = !negated,
                                "fallback-x11" => permissions.fallback_x11 = !negated,
                                "pulseaudio" => permissions.pulseaudio = !negated,
                                _ => {}
                            }
                        }
                    }
                    "devices" => {
                        for item in items {
                            let (negated, name) = if item.starts_with('!') { (true, &item[1..]) } else { (false, item) };
                            if name == "dri" {
                                permissions.gpu = !negated;
                            }
                        }
                    }
                    "filesystems" => {
                        for item in items {
                            let (negated, name) = if item.starts_with('!') { (true, &item[1..]) } else { (false, item) };
                            if name == "host" {
                                permissions.host_files = !negated;
                            } else if name == "home" {
                                permissions.home_files = !negated;
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
    }
}

#[tauri::command]
pub async fn get_flatpak_permissions(app_id: String) -> Result<FlatpakPermissions, String> {
    if !crate::commands::flatpak_rpm::is_safe_package_id(&app_id) {
        return Err("Invalid Flatpak application ID".to_string());
    }
    let mut perms = FlatpakPermissions {
        network: false,
        ipc: false,
        fallback_x11: false,
        x11: false,
        wayland: false,
        pulseaudio: false,
        gpu: false,
        host_files: false,
        home_files: false,
    };

    let mut base_metadata = None;
    let paths = vec![
        format!("/var/lib/flatpak/app/{}/current/active/metadata", app_id),
        format!("/var/lib/flatpak/app/{}/x86_64/stable/active/metadata", app_id),
    ];

    let mut found_path = None;
    for p in paths {
        let path = Path::new(&p);
        if path.exists() {
            found_path = Some(path.to_path_buf());
            break;
        }
    }

    if found_path.is_none() {
        if let Some(home) = dirs::home_dir() {
            let user_paths = vec![
                home.join(format!(".local/share/flatpak/app/{}/current/active/metadata", app_id)),
                home.join(format!(".local/share/flatpak/app/{}/x86_64/stable/active/metadata", app_id)),
            ];
            for p in user_paths {
                if p.exists() {
                    found_path = Some(p);
                    break;
                }
            }
        }
    }

    if let Some(path) = found_path {
        if let Ok(content) = fs::read_to_string(path) {
            base_metadata = Some(content);
        }
    }

    if base_metadata.is_none() {
        if let Ok(output) = Command::new("flatpak")
            .args(["info", "--show-metadata", &app_id])
            .output()
            .await
        {
            if output.status.success() {
                let content = String::from_utf8_lossy(&output.stdout).to_string();
                base_metadata = Some(content);
            }
        }
    }

    if let Some(content) = base_metadata {
        parse_flatpak_ini(&content, &mut perms, false);
    }

    if let Some(home) = dirs::home_dir() {
        let override_path = home.join(format!(".local/share/flatpak/overrides/{}", app_id));
        if override_path.exists() {
            if let Ok(content) = fs::read_to_string(override_path) {
                parse_flatpak_ini(&content, &mut perms, true);
            }
        }
    }

    Ok(perms)
}

#[tauri::command]
pub async fn set_flatpak_permission(
    app_id: String,
    permission: String,
    enable: bool,
) -> Result<(), String> {
    if !crate::commands::flatpak_rpm::is_safe_package_id(&app_id) {
        return Err("Invalid Flatpak application ID".to_string());
    }
    let mut args = vec!["override", "--user"];
    
    let opt = match permission.as_str() {
        "network" => if enable { "--share=network" } else { "--unshare=network" },
        "ipc" => if enable { "--share=ipc" } else { "--unshare=ipc" },
        "x11" => if enable { "--socket=x11" } else { "--nosocket=x11" },
        "wayland" => if enable { "--socket=wayland" } else { "--nosocket=wayland" },
        "fallback_x11" => if enable { "--socket=fallback-x11" } else { "--nosocket=fallback-x11" },
        "pulseaudio" => if enable { "--socket=pulseaudio" } else { "--nosocket=pulseaudio" },
        "gpu" => if enable { "--device=dri" } else { "--nodevice=dri" },
        "host_files" => if enable { "--filesystem=host" } else { "--nofilesystem=host" },
        "home_files" => if enable { "--filesystem=home" } else { "--nofilesystem=home" },
        _ => return Err(format!("Unknown permission type: {}", permission)),
    };
    
    args.push(opt);
    args.push(&app_id);

    let output = Command::new("flatpak")
        .args(&args)
        .output()
        .await
        .map_err(|e| format!("Failed to run flatpak override: {e}"))?;

    if !output.status.success() {
        let err = String::from_utf8_lossy(&output.stderr).to_string();
        return Err(format!("flatpak override failed: {err}"));
    }

    Ok(())
}

#[tauri::command]
pub async fn get_app_dependencies(package_id: String, source: String) -> Result<Vec<String>, String> {
    if source != "Flatpak" && source != "RPM" {
        return Ok(Vec::new());
    }
    if !crate::commands::flatpak_rpm::is_safe_package_id(&package_id) {
        return Err("Invalid application package ID".to_string());
    }
    let mut deps = Vec::new();
    if source == "Flatpak" {
        if let Ok(output) = Command::new("flatpak")
            .arg("info")
            .arg(&package_id)
            .output()
            .await
        {
            if output.status.success() {
                let stdout = String::from_utf8_lossy(&output.stdout);
                for line in stdout.lines() {
                    let line = line.trim();
                    if line.starts_with("Runtime:") || line.starts_with("Sdk:") || line.starts_with("Ref:") {
                        deps.push(line.to_string());
                    }
                }
            }
        }
    } else {
        if let Ok(output) = Command::new("rpm")
            .arg("-q")
            .arg("--requires")
            .arg(&package_id)
            .output()
            .await
        {
            if output.status.success() {
                let stdout = String::from_utf8_lossy(&output.stdout);
                for line in stdout.lines() {
                    let line = line.trim();
                    if !line.starts_with("rpmlib(") && !line.is_empty() {
                        deps.push(line.to_string());
                    }
                }
            }
        }
        if deps.is_empty() && is_debian_based() {
            if let Ok(output) = Command::new("dpkg-query")
                .arg("-W")
                .arg("-f=${Depends}")
                .arg(&package_id)
                .output()
                .await
            {
                if output.status.success() {
                    let stdout = String::from_utf8_lossy(&output.stdout);
                    for dep in stdout.split(',') {
                        let d = dep.trim();
                        if !d.is_empty() {
                            deps.push(d.to_string());
                        }
                    }
                }
            }
        }
    }
    
    Ok(deps)
}

fn appimage_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Some(home) = dirs::home_dir() {
        dirs.push(home.join("Applications"));
        dirs.push(home.join(".local/bin"));
    }
    dirs
}

/// One naming rule for the desktop entry and for the "has shortcut" flag.
fn appimage_desktop_filename(stem: &str) -> String {
    format!("appimage-{}.desktop", stem.replace(' ', "-").to_lowercase())
}

/// Only AppImages directly inside the directories the scanner reads may be
/// acted on, and only after resolving symlinks. A crafted path can not point
/// the delete or shortcut action at an arbitrary file.
fn is_safe_appimage_path(path: &Path) -> bool {
    let Some(name) = path.file_name().and_then(|value| value.to_str()) else {
        return false;
    };
    if !name.to_ascii_lowercase().ends_with(".appimage") {
        return false;
    }
    let Ok(resolved) = path.canonicalize() else {
        return false;
    };
    appimage_dirs()
        .iter()
        .filter_map(|dir| dir.canonicalize().ok())
        .any(|dir| resolved.parent() == Some(dir.as_path()))
}

fn appimage_cache_path(home: &Path, stem: &str, extension: &str) -> PathBuf {
    home.join(".local/share/icons")
        .join(format!("lcp-appimage-{stem}.{extension}"))
}

fn cached_appimage_icon(home: &Path, stem: &str) -> Option<String> {
    for extension in ["png", "svg", "xpm"] {
        let path = appimage_cache_path(home, stem, extension);
        if path.is_file() {
            return Some(path.to_string_lossy().to_string());
        }
    }
    None
}

/// The icon without extracting anything: reading `.DirIcon` means running the
/// AppImage, which only the explicit menu-entry action may do. Without a
/// cached or sibling icon the row keeps its category icon.
fn appimage_quick_icon(path: &Path) -> Option<String> {
    let home = dirs::home_dir()?;
    let stem = path.file_stem()?.to_string_lossy().to_string();
    let icon = cached_appimage_icon(&home, &stem)
        .map(PathBuf::from)
        .or_else(|| {
            let parent = path.parent()?;
            ["png", "svg", "jpg", "jpeg", "webp"]
                .iter()
                .map(|extension| parent.join(format!("{stem}.{extension}")))
                .find(|candidate| candidate.is_file())
        })?;
    icon_data_url(&icon)
}

/// An AppImage is an ELF file with the AppImage magic at offset 8. Checking it
/// before running `--appimage-extract` matters: a file that merely ends in
/// `.AppImage` would otherwise be executed as a normal program.
fn is_appimage_binary(path: &Path) -> bool {
    use std::io::Read;
    let Ok(mut file) = fs::File::open(path) else {
        return false;
    };
    let mut header = [0u8; 11];
    if file.read_exact(&mut header).is_err() {
        return false;
    }
    &header[0..4] == b"\x7fELF" && (&header[8..11] == b"AI\x01" || &header[8..11] == b"AI\x02")
}

async fn extract_appimage_path(path: &Path, working_dir: &Path, pattern: &str) -> bool {
    use std::process::Stdio;
    let mut command = tokio::process::Command::new(path);
    command
        .arg("--appimage-extract")
        .arg(pattern)
        .current_dir(working_dir)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .kill_on_drop(true);
    matches!(
        tokio::time::timeout(std::time::Duration::from_secs(20), command.output()).await,
        Ok(Ok(_))
    )
}

/// Read the icon an AppImage ships in its AppDir and copy it where the desktop
/// entry can point at it. `.DirIcon` is normally a symlink into
/// `usr/share/icons/…`, so extract the target as a second pass when needed.
async fn extract_appimage_icon(path: &Path, home: &Path) -> Option<String> {
    if !is_appimage_binary(path) {
        return None;
    }
    let stem = path.file_stem()?.to_string_lossy().to_string();
    let working_dir = std::env::temp_dir().join(format!(
        "lcp-appimage-icon-{}-{}",
        std::process::id(),
        stem.replace([' ', '/'], "-")
    ));
    let _ = fs::remove_dir_all(&working_dir);
    fs::create_dir_all(&working_dir).ok()?;

    let result = async {
        if !extract_appimage_path(path, &working_dir, ".DirIcon").await {
            return None;
        }
        let extracted = working_dir.join("squashfs-root/.DirIcon");
        let mut icon_file = if extracted.is_file() {
            Some(extracted.clone())
        } else {
            None
        };

        if icon_file.is_none() {
            let target = fs::read_link(&extracted).ok()?;
            if target.is_absolute() || target.components().any(|part| part.as_os_str() == "..") {
                return None;
            }
            let target_text = target.to_string_lossy().to_string();
            if !extract_appimage_path(path, &working_dir, &target_text).await {
                return None;
            }
            let resolved = working_dir.join("squashfs-root").join(&target);
            if resolved.is_file() {
                icon_file = Some(resolved);
            }
        }

        let icon_file = icon_file?;
        let extension = icon_file
            .extension()
            .map(|value| value.to_string_lossy().to_lowercase())
            .filter(|value| matches!(value.as_str(), "png" | "svg" | "xpm"))
            .unwrap_or_else(|| "png".to_string());
        let cache_dir = home.join(".local/share/icons");
        fs::create_dir_all(&cache_dir).ok()?;
        let destination = appimage_cache_path(home, &stem, &extension);
        fs::copy(&icon_file, &destination).ok()?;
        Some(destination.to_string_lossy().to_string())
    }
    .await;

    let _ = fs::remove_dir_all(&working_dir);
    result
}

async fn appimage_icon(path: &Path, home: &Path, force: bool) -> Option<String> {
    let stem = path.file_stem()?.to_string_lossy().to_string();
    if !force {
        if let Some(cached) = cached_appimage_icon(home, &stem) {
            return Some(cached);
        }
    }

    // An icon shipped next to the AppImage is already usable without running it.
    if let Some(parent) = path.parent() {
        for extension in ["png", "svg", "xpm"] {
            let sibling = parent.join(format!("{stem}.{extension}"));
            if sibling.is_file() {
                let cache_dir = home.join(".local/share/icons");
                fs::create_dir_all(&cache_dir).ok()?;
                let destination = appimage_cache_path(home, &stem, extension);
                fs::copy(&sibling, &destination).ok()?;
                return Some(destination.to_string_lossy().to_string());
            }
        }
    }

    if force {
        // Re-reading is a refresh: keep the previous icon when the AppImage no
        // longer yields one.
        if let Some(icon) = extract_appimage_icon(path, home).await {
            return Some(icon);
        }
        return cached_appimage_icon(home, &stem);
    }
    extract_appimage_icon(path, home).await
}

/// Copy an icon the user picked into the same cache the scan reads first, so
/// the choice survives a reload even when the desktop entry also carries a
/// themed icon.
fn cache_custom_appimage_icon(icon_path: &Path, home: &Path, stem: &str) {
    if icon_mime(icon_path).is_none() {
        return;
    }
    let Some(extension) = icon_path
        .extension()
        .and_then(|value| value.to_str())
        .map(|value| value.to_ascii_lowercase())
    else {
        return;
    };
    let destination = appimage_cache_path(home, stem, &extension);
    if icon_path == destination.as_path() {
        return;
    }
    let Some(cache_dir) = destination.parent() else {
        return;
    };
    if fs::create_dir_all(cache_dir).is_err() {
        return;
    }
    // A previous cache with another extension would win the lookup order.
    remove_cached_appimage_icon(home, stem);
    let _ = fs::copy(icon_path, &destination);
}

/// Version-looking run in an AppImage file name, e.g. `0.13.0`.
fn appimage_version(stem: &str) -> String {
    use std::sync::OnceLock;

    static VERSION_RE: OnceLock<regex::Regex> = OnceLock::new();
    let pattern = VERSION_RE.get_or_init(|| regex::Regex::new(r"\d+(?:\.\d+)+").unwrap());
    pattern
        .find(stem)
        .map(|matched| matched.as_str().to_string())
        .unwrap_or_default()
}

fn file_modified_seconds(path: &Path) -> u64 {
    fs::metadata(path)
        .ok()
        .and_then(|meta| meta.modified().ok())
        .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}

/// User desktop entries whose Exec launches `appimage`. Only the user's own
/// applications directory is read: those entries are safe to update and remove,
/// while a system-wide entry is not ours to touch.
fn appimage_entries_in(dir: &Path, appimage: &Path) -> Vec<(PathBuf, ParsedEntry)> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };
    let target = appimage
        .canonicalize()
        .unwrap_or_else(|_| appimage.to_path_buf());
    let mut matches = Vec::new();

    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() || path.extension().and_then(|value| value.to_str()) != Some("desktop")
        {
            continue;
        }
        let Some(parsed) = parse_desktop_entry(&path) else {
            continue;
        };
        let Ok(argv) = tokenize_exec(&parsed.exec) else {
            continue;
        };
        let Some(program) = argv.first() else {
            continue;
        };
        let program_path = PathBuf::from(program);
        let resolved = program_path
            .canonicalize()
            .unwrap_or_else(|_| program_path.clone());
        if resolved == target {
            matches.push((path, parsed));
        }
    }
    matches
}

/// The entry a merged AppImage row should display. Another tool's entry carries
/// a friendlier name and icon than the panel's own generated file, so it wins
/// when both exist.
fn preferred_appimage_entry(
    dir: &Path,
    appimage: &Path,
    panel_filename: &str,
) -> Option<(PathBuf, ParsedEntry)> {
    let mut matches = appimage_entries_in(dir, appimage);
    matches.sort_by(|left, right| {
        let left_panel = left
            .0
            .file_name()
            .is_some_and(|name| name == panel_filename);
        let right_panel = right
            .0
            .file_name()
            .is_some_and(|name| name == panel_filename);
        left_panel.cmp(&right_panel).then_with(|| left.0.cmp(&right.0))
    });
    matches.into_iter().next()
}

/// Replace the first `Icon=` in an existing desktop entry, or add one to the
/// desktop group when the entry has none. Used when the user picks a custom
/// icon for an AppImage that already has a launcher.
fn set_desktop_icon(path: &Path, icon: &str) -> Result<(), String> {
    let content = fs::read_to_string(path)
        .map_err(|e| format!("Could not read the desktop entry: {e}"))?;
    let mut lines: Vec<String> = content.lines().map(str::to_string).collect();
    let mut replaced = false;
    for line in lines.iter_mut() {
        if line.trim_start().starts_with("Icon=") {
            *line = format!("Icon={icon}");
            replaced = true;
            break;
        }
    }
    if !replaced {
        let index = lines
            .iter()
            .position(|line| line.trim() == "[Desktop Entry]")
            .map(|position| position + 1)
            .unwrap_or(0);
        lines.insert(index, format!("Icon={icon}"));
    }
    fs::write(path, lines.join("\n") + "\n")
        .map_err(|e| format!("Could not update the desktop entry: {e}"))
}

fn remove_cached_appimage_icon(home: &Path, stem: &str) {
    for extension in ["png", "svg", "xpm"] {
        let _ = fs::remove_file(appimage_cache_path(home, stem, extension));
    }
}

/// Values end up inside a `.desktop` key. A newline would start a new key, so
/// it is refused rather than escaped.
fn desktop_value(value: &str) -> Result<String, String> {
    if value.trim().is_empty() {
        return Err("The value must not be empty.".to_string());
    }
    if value.contains(['\n', '\r']) {
        return Err("The value must not contain line breaks.".to_string());
    }
    Ok(value.trim().to_string())
}

fn desktop_icon_value(value: &str) -> Result<String, String> {
    let value = desktop_value(value)?;
    if value.contains('/') {
        let path = Path::new(&value);
        if !path.is_absolute() || !path.is_file() {
            return Err("The icon path must point at an existing file.".to_string());
        }
    }
    Ok(value)
}

fn escape_exec_value(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('%', "%%")
}

/// Make the new entry visible in the menu without waiting for the next login.
async fn refresh_desktop_database(home: &Path) {
    let directory = home.join(".local/share/applications");
    let _ = tokio::process::Command::new("update-desktop-database")
        .arg(&directory)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .output()
        .await;
}

#[tauri::command]
pub async fn scan_local_appimages() -> Result<Vec<DesktopApp>, String> {
    let mut apps = Vec::new();
    let home = dirs::home_dir();
    let applications_dir = home
        .as_ref()
        .map(|home| home.join(".local/share/applications"));
    let mut icon_index: Option<IconIndex> = None;
    let mut icon_cache: HashMap<PathBuf, Option<String>> = HashMap::new();
    let empty_index = IconIndex {
        by_name: HashMap::new(),
    };

    for dir in appimage_dirs() {
        let Ok(entries) = fs::read_dir(dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            let Some(filename) = path.file_name().and_then(|value| value.to_str()) else {
                continue;
            };
            if !filename.to_ascii_lowercase().ends_with(".appimage") {
                continue;
            }
            let path_text = path.to_string_lossy().to_string();
            let stem = path
                .file_stem()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();
            let panel_filename = appimage_desktop_filename(&stem);

            // A launcher another tool (or the panel itself) already made for
            // this file supplies the real name and icon, and lets the row act
            // on the entry instead of creating a second one.
            let matched = applications_dir
                .as_deref()
                .and_then(|dir| preferred_appimage_entry(dir, &path, &panel_filename));
            let (name, package_id, desktop_path, entry_icon) = match matched {
                Some((desktop_path, parsed)) => (
                    parsed.name,
                    desktop_path
                        .file_stem()
                        .map(|value| value.to_string_lossy().to_string()),
                    Some(desktop_path.to_string_lossy().to_string()),
                    parsed.icon,
                ),
                None => (stem.clone(), Some(String::new()), None, None),
            };

            // The cache and a sibling icon are the AppImage's own artwork and
            // must win: a custom icon or a freshly read icon is stored there.
            let icon = appimage_quick_icon(&path).or_else(|| {
                let value = entry_icon?;
                let index: &IconIndex = if value.contains('/') {
                    &empty_index
                } else {
                    icon_index.get_or_insert_with(IconIndex::build)
                };
                resolve_icon_data(Some(&value), index, &mut icon_cache)
            });

            apps.push(DesktopApp {
                name,
                exec: path_text.clone(),
                source: "AppImage".to_string(),
                package_id,
                file_path: path_text,
                size_bytes: fs::metadata(&path).map(|meta| meta.len()).unwrap_or(0),
                removable: true,
                hidden: false,
                icon,
                desktop_path,
                version: appimage_version(&stem),
                install_date: file_modified_seconds(&path),
            });
        }
    }

    apps.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    Ok(apps)
}

#[tauri::command]
pub async fn register_appimage(
    name: String,
    exec_path: String,
    icon: Option<String>,
    create_shortcut: bool,
) -> Result<(), String> {
    let home = dirs::home_dir().ok_or_else(|| "Could not find home directory".to_string())?;
    let path = PathBuf::from(&exec_path);
    if !is_safe_appimage_path(&path) {
        return Err(
            "Refused: only AppImages in ~/Applications or ~/.local/bin can be added to the menu."
                .to_string(),
        );
    }
    let name = desktop_value(&name)?;

    let desktop_dir = home.join(".local/share/applications");
    fs::create_dir_all(&desktop_dir)
        .map_err(|e| format!("Could not create the application directory: {e}"))?;
    let stem = path
        .file_stem()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();
    let desktop_filepath = desktop_dir.join(appimage_desktop_filename(&stem));
    let existing = appimage_entries_in(&desktop_dir, &path);

    if create_shortcut {
        if let Some(value) = icon {
            // Picking an icon updates the launcher that is already there rather
            // than adding a second one next to it.
            let value = desktop_icon_value(&value)?;
            if existing.is_empty() {
                write_appimage_entry(&desktop_filepath, &name, &exec_path, &value)?;
            } else {
                for (entry_path, _) in &existing {
                    set_desktop_icon(entry_path, &value)?;
                }
            }
            let custom = Path::new(&value);
            if custom.is_absolute() {
                cache_custom_appimage_icon(custom, &home, &stem);
            }
        } else if existing.is_empty() {
            let icon = appimage_icon(&path, &home, false)
                .await
                .unwrap_or_else(|| "system-run".to_string());
            write_appimage_entry(&desktop_filepath, &name, &exec_path, &icon)?;
        }
        // No icon argument and an existing launcher: it is already in the menu.
    } else {
        for (entry_path, _) in &existing {
            fs::remove_file(entry_path)
                .map_err(|e| format!("Could not remove the menu entry: {e}"))?;
        }
        // A panel entry whose Exec could not be parsed is still ours to remove.
        if existing.is_empty() && desktop_filepath.exists() {
            fs::remove_file(&desktop_filepath)
                .map_err(|e| format!("Could not remove the desktop entry: {e}"))?;
        }
    }

    refresh_desktop_database(&home).await;
    Ok(())
}

fn write_appimage_entry(
    path: &Path,
    name: &str,
    exec_path: &str,
    icon: &str,
) -> Result<(), String> {
    let content = format!(
        "[Desktop Entry]\nType=Application\nName={}\nExec=\"{}\"\nIcon={}\nComment=AppImage application\nTerminal=false\nCategories=Utility;\nStartupNotify=true\n",
        name,
        escape_exec_value(exec_path),
        icon
    );
    fs::write(path, content).map_err(|e| format!("Failed to write the desktop entry: {e}"))
}

/// Run the AppImage once to read its `.DirIcon`, on an explicit user action.
/// `force` re-reads an icon that was already cached, so a refreshed AppImage
/// does not keep stale artwork.
#[tauri::command]
pub async fn read_appimage_icon(
    exec_path: String,
    force: bool,
) -> Result<Option<String>, String> {
    let path = PathBuf::from(&exec_path);
    if !is_safe_appimage_path(&path) {
        return Err(
            "Refused: only AppImages in ~/Applications or ~/.local/bin can be read here."
                .to_string(),
        );
    }
    if !path.is_file() {
        return Err("The AppImage no longer exists. Refresh the list.".to_string());
    }
    let home = dirs::home_dir().ok_or_else(|| "Could not find home directory".to_string())?;
    let icon = appimage_icon(&path, &home, force).await;

    // A launcher that already exists should show the icon the user just read,
    // not keep its old themed one.
    if let Some(icon_path) = icon.as_deref() {
        let desktop_dir = home.join(".local/share/applications");
        let mut changed = false;
        for (entry_path, _) in appimage_entries_in(&desktop_dir, &path) {
            if set_desktop_icon(&entry_path, icon_path).is_ok() {
                changed = true;
            }
        }
        if changed {
            refresh_desktop_database(&home).await;
        }
    }

    Ok(icon.and_then(|icon| icon_data_url(Path::new(&icon))))
}

/// Delete an AppImage and its menu entry. The file is user-owned, so this does
/// not need the privileged helper.
#[tauri::command]
pub async fn delete_appimage(exec_path: String) -> Result<(), String> {
    let path = PathBuf::from(&exec_path);
    if !is_safe_appimage_path(&path) {
        return Err(
            "Refused: only AppImages in ~/Applications or ~/.local/bin can be deleted here."
                .to_string(),
        );
    }
    if !path.is_file() {
        return Err("The AppImage no longer exists. Refresh the list.".to_string());
    }
    let home = dirs::home_dir().ok_or_else(|| "Could not find home directory".to_string())?;
    let stem = path
        .file_stem()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();

    let desktop_dir = home.join(".local/share/applications");
    for (entry_path, _) in appimage_entries_in(&desktop_dir, &path) {
        fs::remove_file(&entry_path)
            .map_err(|e| format!("Could not remove the menu entry: {e}"))?;
    }
    // The panel's own entry when its Exec no longer parses.
    let desktop_file = desktop_dir.join(appimage_desktop_filename(&stem));
    if desktop_file.exists() {
        fs::remove_file(&desktop_file)
            .map_err(|e| format!("Could not remove the menu entry: {e}"))?;
    }

    fs::remove_file(&path).map_err(|e| format!("Could not delete the AppImage: {e}"))?;
    remove_cached_appimage_icon(&home, &stem);
    refresh_desktop_database(&home).await;
    Ok(())
}

/// Directories the panel itself scans for `.desktop` entries. A launch request
/// may only name a file inside one of them, so a crafted path can not make the
/// panel execute a downloaded entry from `~/Downloads`.
fn desktop_entry_dirs() -> Vec<std::path::PathBuf> {
    let mut dirs = vec![
        std::path::PathBuf::from("/usr/share/applications"),
        std::path::PathBuf::from("/var/lib/flatpak/exports/share/applications"),
    ];
    if let Some(home) = dirs::home_dir() {
        dirs.push(home.join(".local/share/applications"));
    }
    dirs
}

/// Read the main `Exec=` line out of a `.desktop` file.
fn read_desktop_exec(path: &std::path::Path) -> Result<String, String> {
    let text = std::fs::read_to_string(path)
        .map_err(|e| format!("Could not read the desktop entry: {e}"))?;
    text.lines()
        .map(str::trim_start)
        .find_map(|line| line.strip_prefix("Exec="))
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "the desktop entry has no Exec= line".to_string())
}

/// Split an `Exec=` line into argv the way the desktop entry spec defines it:
/// double quotes group, a backslash escapes the next character, and `%` field
/// codes are not arguments at all (`%%` is a literal percent).
///
/// There is deliberately no shell here: `;`, `|`, backticks and `$(…)` are
/// ordinary characters, which is the whole point of tokenising instead of
/// handing the line to `sh -c`.
fn tokenize_exec(exec: &str) -> Result<Vec<String>, String> {
    let mut argv: Vec<String> = Vec::new();
    let mut current = String::new();
    let mut started = false;
    let mut in_quotes = false;
    let mut chars = exec.chars();

    while let Some(c) = chars.next() {
        match c {
            '"' => {
                in_quotes = !in_quotes;
                started = true;
            }
            '\\' => {
                if let Some(escaped) = chars.next() {
                    current.push(escaped);
                    started = true;
                }
            }
            _ if c.is_whitespace() && !in_quotes => {
                if started {
                    argv.push(std::mem::take(&mut current));
                    started = false;
                }
            }
            '%' => match chars.next() {
                Some('%') => {
                    current.push('%');
                    started = true;
                }
                // %f %F %u %U %i %c %k … are substituted by the caller, never argv.
                Some(_) | None => {}
            },
            _ => {
                current.push(c);
                started = true;
            }
        }
    }

    if in_quotes {
        return Err("the Exec line has an unterminated quote".to_string());
    }
    if started {
        argv.push(current);
    }
    if argv.is_empty() {
        return Err("the Exec line is empty".to_string());
    }
    Ok(argv)
}

/// Launch an installed application.
///
/// The webview names the *file*, never the command: for a `.desktop` entry the
/// `Exec=` line is read from disk here, and for a local binary the file itself
/// is the program. Either way the result is spawned as argv with no shell, so
/// a compromised renderer can only start something that is really installed.
#[tauri::command]
pub async fn launch_desktop_app(file_path: String) -> Result<(), String> {
    crate::utils::root_policy::ensure_openable_path(&file_path)?;
    let path = std::path::Path::new(&file_path);

    let argv = if path.extension().is_some_and(|ext| ext == "desktop") {
        if !desktop_entry_dirs().iter().any(|dir| path.starts_with(dir)) {
            return Err(format!(
                "Refused to launch a desktop entry outside the application directories: {file_path}"
            ));
        }
        tokenize_exec(&read_desktop_exec(path)?)?
    } else {
        // AppImage and other local binaries: the file itself is the program.
        vec![file_path.clone()]
    };

    let program = argv[0].clone();
    tokio::process::Command::new(&program)
        .args(&argv[1..])
        .spawn()
        .map_err(|e| format!("Failed to launch {program}: {e}"))?;

    Ok(())
}

#[tauri::command]
pub async fn reveal_in_file_manager(path: String) -> Result<(), String> {
    crate::utils::root_policy::ensure_openable_path(&path)?;
    let abs_path = std::path::PathBuf::from(&path);

    if abs_path.is_file() {
        let uri = format!("file://{}", abs_path.to_string_lossy());
        
        // 1. Try DBus org.freedesktop.FileManager1 ShowItems (standard Linux Desktop spec)
        if let Ok(output) = tokio::process::Command::new("dbus-send")
            .args([
                "--session",
                "--dest=org.freedesktop.FileManager1",
                "--type=method_call",
                "/org/freedesktop/FileManager1",
                "org.freedesktop.FileManager1.ShowItems",
                &format!("array:string:{}", uri),
                "string:",
            ])
            .output()
            .await
        {
            if output.status.success() {
                return Ok(());
            }
        }

        // 2. Try nautilus --select
        if tokio::process::Command::new("nautilus")
            .arg("--select")
            .arg(&abs_path)
            .spawn()
            .is_ok()
        {
            return Ok(());
        }

        // 3. Fallback: xdg-open parent folder
        let parent = abs_path.parent().unwrap_or(&abs_path);
        tokio::process::Command::new("xdg-open")
            .arg(parent)
            .spawn()
            .map_err(|e| format!("Failed to open file manager: {e}"))?;
    } else {
        tokio::process::Command::new("xdg-open")
            .arg(&abs_path)
            .spawn()
            .map_err(|e| format!("Failed to open file manager: {e}"))?;
    }

    Ok(())
}

#[cfg(test)]
mod scan_tests {
    use super::{
        appimage_entries_in, appimage_version, icon_data_url, icon_mime, icon_size_priority,
        parse_desktop_entry, preferred_appimage_entry, set_desktop_icon, IconIndex,
    };

    fn test_dir(label: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "lcp-desktop-scan-{}-{}",
            std::process::id(),
            label
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn appimage_versions_come_from_the_file_name() {
        assert_eq!(
            appimage_version("craft-anything-0.6.1-linux-x86_64"),
            "0.6.1"
        );
        assert_eq!(
            appimage_version("Freebuff-0.0.192-linux-x86_64"),
            "0.0.192"
        );
        assert_eq!(appimage_version("tool-AppImage"), "");
    }

    #[test]
    fn desktop_entries_are_matched_by_their_exec_program() {
        let dir = test_dir("appimage-entries");
        let appimage = dir.join("Demo-1.2.3.AppImage");
        std::fs::write(&appimage, b"not really an AppImage").unwrap();
        let applications = dir.join("applications");
        std::fs::create_dir_all(&applications).unwrap();

        let matching = applications.join("demo.desktop");
        std::fs::write(
            &matching,
            format!(
                "[Desktop Entry]\nType=Application\nName=Demo\nExec=\"{}\" %F\n",
                appimage.display()
            ),
        )
        .unwrap();
        std::fs::write(
            applications.join("other.desktop"),
            "[Desktop Entry]\nType=Application\nName=Other\nExec=/usr/bin/other\n",
        )
        .unwrap();
        // A path in a comment must not count as a launcher.
        std::fs::write(
            applications.join("comment.desktop"),
            format!(
                "[Desktop Entry]\nType=Application\nName=Comment\nExec=/usr/bin/other\nComment={}\n",
                appimage.display()
            ),
        )
        .unwrap();

        let matches = appimage_entries_in(&applications, &appimage);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].0, matching);
        assert_eq!(matches[0].1.name, "Demo");

        // A launcher another tool made is preferred over the panel's own file.
        let panel_filename = "appimage-demo-1.2.3.desktop";
        std::fs::write(
            applications.join(panel_filename),
            format!(
                "[Desktop Entry]\nType=Application\nName=Panel Demo\nExec=\"{}\"\n",
                appimage.display()
            ),
        )
        .unwrap();
        let preferred = preferred_appimage_entry(&applications, &appimage, panel_filename)
            .expect("an entry is chosen");
        assert_eq!(preferred.0, matching);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_custom_icon_updates_an_existing_desktop_entry() {
        let dir = test_dir("set-icon");
        let entry = dir.join("demo.desktop");
        std::fs::write(
            &entry,
            "[Desktop Entry]\nType=Application\nName=Demo\nExec=demo\nIcon=old-theme-icon\nComment=Kept\n",
        )
        .unwrap();

        set_desktop_icon(&entry, "/tmp/picked.png").unwrap();
        let content = std::fs::read_to_string(&entry).unwrap();
        assert!(content.contains("Icon=/tmp/picked.png"));
        assert!(!content.contains("Icon=old-theme-icon"));
        assert!(content.contains("Comment=Kept"));

        // An entry without an Icon line gets one inside its desktop group.
        let plain = dir.join("plain.desktop");
        std::fs::write(
            &plain,
            "[Desktop Entry]\nType=Application\nName=Plain\nExec=plain\n",
        )
        .unwrap();
        set_desktop_icon(&plain, "/tmp/picked.png").unwrap();
        let content = std::fs::read_to_string(&plain).unwrap();
        assert!(content.contains("[Desktop Entry]\nIcon=/tmp/picked.png"));
        assert_eq!(content.matches("Icon=").count(), 1);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn icon_key_is_read_from_the_entry() {
        let dir = std::env::temp_dir().join(format!(
            "lcp-desktop-scan-{}-{}",
            std::process::id(),
            "icon"
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("with-icon.desktop");
        std::fs::write(
            &path,
            "[Desktop Entry]\nType=Application\nName=With Icon\nExec=app\nIcon=org.example.App\n",
        )
        .unwrap();

        let entry = parse_desktop_entry(&path).expect("entry is parsed");
        assert_eq!(entry.icon.as_deref(), Some("org.example.App"));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn icon_sizes_rank_and_symbolic_sets_are_skipped() {
        assert_eq!(icon_size_priority("64x64"), Some(0));
        assert!(icon_size_priority("48x48") < icon_size_priority("256x256"));
        // HiDPI variants rank by their base directory.
        assert_eq!(icon_size_priority("64x64@2"), Some(0));
        assert_eq!(icon_size_priority("symbolic"), None);
    }

    #[test]
    fn icons_are_resolved_by_name_and_inlined_with_their_mime_type() {
        let dir = std::env::temp_dir().join(format!(
            "lcp-desktop-scan-{}-{}",
            std::process::id(),
            "resolve"
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let icon_path = dir.join("demo.png");
        std::fs::write(&icon_path, b"png-bytes").unwrap();
        let xpm_path = dir.join("legacy.xpm");
        std::fs::write(&xpm_path, b"xpm-bytes").unwrap();

        let mut index = IconIndex {
            by_name: std::collections::HashMap::new(),
        };
        index.by_name.insert("demo".to_string(), (0, icon_path.clone()));
        // Dotted application IDs keep every dot; only a known image extension
        // is stripped from the reference.
        index
            .by_name
            .insert("org.example.app".to_string(), (0, icon_path.clone()));

        assert_eq!(index.resolve("demo"), Some(icon_path.clone()));
        assert_eq!(index.resolve("Demo"), Some(icon_path.clone()));
        assert_eq!(index.resolve("demo.png"), Some(icon_path.clone()));
        assert_eq!(
            index.resolve("org.example.App"),
            Some(icon_path.clone())
        );
        assert_eq!(index.resolve("org.example"), None);
        // An absolute Icon= works without the index, a relative one does not.
        assert_eq!(
            index.resolve(&icon_path.to_string_lossy()),
            Some(icon_path.clone())
        );
        assert_eq!(index.resolve("missing"), None);

        let data = icon_data_url(&icon_path).expect("png is inlineable");
        assert!(data.starts_with("data:image/png;base64,"));
        // XPM cannot be drawn by the webview, so it falls back to the category
        // icon instead of producing a broken image.
        assert_eq!(icon_mime(&xpm_path), None);
        assert_eq!(icon_data_url(&xpm_path), None);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn hidden_desktop_entries_are_flagged_instead_of_dropped() {
        let dir = std::env::temp_dir().join(format!(
            "lcp-desktop-scan-{}-{}",
            std::process::id(),
            "hidden"
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let visible_path = dir.join("visible.desktop");
        std::fs::write(
            &visible_path,
            "[Desktop Entry]\nType=Application\nName=Visible\nExec=visible %U\n",
        )
        .unwrap();
        let hidden_path = dir.join("hidden.desktop");
        std::fs::write(
            &hidden_path,
            "[Desktop Entry]\nType=Application\nName=Hidden\nExec=hidden\nNoDisplay=true\n",
        )
        .unwrap();

        let visible = parse_desktop_entry(&visible_path).expect("visible entry is parsed");
        assert!(!visible.hidden);
        assert_eq!(visible.name, "Visible");

        // The filter now happens in the list, not while parsing: a package
        // whose only launcher is hidden must still be reachable.
        let hidden = parse_desktop_entry(&hidden_path).expect("hidden entry is kept");
        assert!(hidden.hidden);
        assert_eq!(hidden.name, "Hidden");

        let _ = std::fs::remove_dir_all(&dir);
    }
}

#[cfg(test)]
mod launch_tests {
    use super::tokenize_exec;

    #[test]
    fn exec_lines_are_tokenised_instead_of_interpreted() {
        assert_eq!(tokenize_exec("firefox").unwrap(), vec!["firefox"]);
        assert_eq!(
            tokenize_exec("env FOO=1 \"my app\" --flag").unwrap(),
            vec!["env", "FOO=1", "my app", "--flag"]
        );
        assert_eq!(
            tokenize_exec(r#"app --name=\"x\""#).unwrap(),
            vec!["app", "--name=\"x\""]
        );

        // Field codes are substituted by the caller; they are never argv.
        assert_eq!(tokenize_exec("firefox %u").unwrap(), vec!["firefox"]);
        assert_eq!(
            tokenize_exec("opener --file=%f").unwrap(),
            vec!["opener", "--file="]
        );
        assert_eq!(tokenize_exec("say 100%%").unwrap(), vec!["say", "100%"]);

        // Shell syntax stays ordinary characters — this is the whole point of
        // not handing the line to `sh -c` any more.
        assert_eq!(
            tokenize_exec("app; rm -rf ~").unwrap(),
            vec!["app;", "rm", "-rf", "~"]
        );
        assert_eq!(
            tokenize_exec("a `id` $(whoami)").unwrap(),
            vec!["a", "`id`", "$(whoami)"]
        );
        assert_eq!(
            tokenize_exec("app && curl http://evil").unwrap(),
            vec!["app", "&&", "curl", "http://evil"]
        );
    }

    #[test]
    fn broken_exec_lines_are_refused() {
        assert!(tokenize_exec("").is_err(), "empty must be refused");
        assert!(tokenize_exec("   ").is_err(), "blank must be refused");
        assert!(
            tokenize_exec("app \"unterminated").is_err(),
            "an unterminated quote must be refused"
        );
    }

    /// The frontend sends `[{ package_id, source }]`; the nested field names
    /// are serde's, not Tauri's camelCase argument mapping, so a rename here
    /// would silently zero every size and version in the table.
    #[test]
    fn meta_requests_deserialize_the_frontend_payload() {
        let items: Vec<super::AppMetaRequest> = serde_json::from_str(
            r#"[{"package_id":"opencode-2.2.7-1.x86_64","source":"RPM"},
                {"package_id":"com.anydesk.Anydesk","source":"Flatpak"}]"#,
        )
        .unwrap();
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].package_id, "opencode-2.2.7-1.x86_64");
        assert_eq!(items[0].source, "RPM");
        assert_eq!(items[1].source, "Flatpak");
    }
}

