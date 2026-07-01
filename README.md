<div align="center">

# 🖥️ Linux Control Panel

**A modern, powerful Linux system management desktop application**

[![Tauri](https://img.shields.io/badge/Tauri-v2-24C8D8?style=for-the-badge&logo=tauri&logoColor=white)](https://tauri.app)
[![Svelte](https://img.shields.io/badge/Svelte-v5-FF3E00?style=for-the-badge&logo=svelte&logoColor=white)](https://svelte.dev)
[![TailwindCSS](https://img.shields.io/badge/TailwindCSS-v4-06B6D4?style=for-the-badge&logo=tailwindcss&logoColor=white)](https://tailwindcss.com)
[![Rust](https://img.shields.io/badge/Rust-1.77+-CE422B?style=for-the-badge&logo=rust&logoColor=white)](https://www.rust-lang.org)
[![Fedora](https://img.shields.io/badge/Fedora-40%2B-294172?style=for-the-badge&logo=fedora&logoColor=white)](https://fedoraproject.org)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg?style=for-the-badge)](LICENSE)

> A blazing-fast, native desktop app for Fedora/RHEL users to manage packages, services, startup entries, repositories, and system files — all from one elegant UI. Built with Tauri v2 (Rust backend) + Svelte 5 frontend.

</div>

---

## ✨ Features

| Module | Description |
|--------|-------------|
| 📦 **Repo Manager** | List, toggle, and add DNF repositories from `/etc/yum.repos.d/` |
| 📜 **DNF History** | Browse transaction history and rollback with `dnf history undo` |
| 🔍 **Copr Browser** | Search and enable/disable Fedora Copr repositories |
| ⚖️ **Flatpak vs RPM** | Detect duplicate packages, compare versions, remove either |
| 🚀 **Startup Manager** | Manage systemd services and XDG autostart entries |
| ⚙️ **Service Manager** | Browse units, start/stop/restart, view logs, and edit unit files |
| 🌐 **Hosts Manager** | Edit `/etc/hosts` with category grouping and inline editing |

---

## 🛠️ Tech Stack

| Layer | Technology |
|-------|-----------|
| **Frontend** | Svelte 5 (Runes), TailwindCSS v4 |
| **Backend** | Rust, Tauri v2 |
| **Editor** | CodeMirror 6 |
| **Icons** | Lucide Svelte |
| **Packaging** | RPM, AppImage |
| **Auth** | PolicyKit (`pkexec`) |

---

## 📋 Prerequisites

### System dependencies (Fedora 40+ / RHEL 9+)

```bash
# Rust toolchain
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source $HOME/.cargo/env

# Tauri system dependencies
sudo dnf install -y \
  webkit2gtk4.1-devel \
  openssl-devel \
  curl \
  wget \
  file \
  libappindicator-gtk3-devel \
  librsvg2-devel

# Node.js (v20+)
sudo dnf install -y nodejs npm
```

### Verify installations

```bash
rustc --version     # >= 1.77
cargo --version
node --version      # >= 20
npm --version
```

---

## 🚀 Installation & Build

```bash
# Clone the repository
git clone https://github.com/your-org/linux-control-panel
cd linux-control-panel

# Install frontend dependencies
npm install

# Development mode (hot reload)
npm run tauri:dev

# Production build (generates .rpm and .AppImage)
npm run tauri:build
```

Build artifacts will be placed in `src-tauri/target/release/bundle/`:
- `rpm/linux-control-panel-*.x86_64.rpm`
- `appimage/linux-control-panel_*.AppImage`

### Install from RPM

```bash
sudo dnf install ./src-tauri/target/release/bundle/rpm/linux-control-panel-*.x86_64.rpm
```

---

## 🔐 Polkit Policy

The app uses PolicyKit for privilege escalation — **no `sudo` required**.

Install the polkit policy to allow privileged operations with a graphical authentication prompt:

```bash
sudo install -m 644 \
  src-tauri/polkit/com.controlpanel.pkexec.policy \
  /usr/share/polkit-1/actions/
```

This allows the Control Panel to perform privileged operations (package management, service control, writing system files) via `pkexec`.

---

## 🏗️ Architecture

```
linux-control-panel/
├── src-tauri/              # Rust backend (Tauri v2)
│   ├── src/
│   │   ├── lib.rs          # App setup, command registration
│   │   └── commands/       # One file per module
│   │       ├── repo_manager.rs
│   │       ├── dnf_history.rs
│   │       ├── copr_browser.rs
│   │       ├── flatpak_rpm.rs
│   │       ├── startup_manager.rs
│   │       ├── service_manager.rs
│   │       └── hosts_manager.rs
│   ├── capabilities/       # Tauri v2 permissions
│   ├── polkit/             # PolicyKit policy file
│   └── tauri.conf.json
├── src/                    # Svelte 5 frontend
│   ├── App.svelte          # Root layout
│   ├── app.css             # TailwindCSS v4 + design system
│   ├── lib/
│   │   ├── stores/         # Svelte 5 rune-based stores
│   │   ├── components/     # Shared UI (Sidebar, Toast, etc.)
│   │   └── modules/        # One component per module
│   └── main.ts
└── package.json
```

---

## 🔒 Security

- **No `sudo`** — all privileged operations use `pkexec` exclusively
- **Polkit authentication** — users authenticate via polkit for each privileged operation class
- **No hardcoded paths** — all binaries are detected at runtime via `which`
- **Async operations** — all shell commands run asynchronously; the UI never blocks
- **Error logging** — all errors are logged to `~/.config/control-panel/logs/control-panel.log`

---

## ⚙️ Config & Logs

| Path | Purpose |
|------|---------|
| `~/.config/control-panel/` | App configuration root |
| `~/.config/control-panel/logs/control-panel.log` | Error/info log file |

---

## 🧑‍💻 Development

```bash
# Frontend only (browser preview)
npm run dev

# Full Tauri dev mode
npm run tauri:dev

# Check Rust compilation without building
cd src-tauri && cargo check

# Run Rust tests
cd src-tauri && cargo test
```

---

## 🐛 Troubleshooting

<details>
<summary><strong>webkit2gtk not found</strong></summary>

```bash
sudo dnf install webkit2gtk4.1-devel
```
</details>

<details>
<summary><strong>pkexec authentication fails</strong></summary>

Make sure the polkit policy is installed:
```bash
ls /usr/share/polkit-1/actions/com.controlpanel.pkexec.policy
```
</details>

<details>
<summary><strong>dnf commands fail</strong></summary>

Ensure you're running on Fedora/RHEL with dnf available:
```bash
which dnf
```
</details>

---

## 🤝 Contributing

Contributions are welcome! Please open an issue first to discuss what you would like to change.

1. Fork the repository
2. Create your feature branch: `git checkout -b feature/my-feature`
3. Commit your changes: `git commit -m 'feat: add my feature'`
4. Push to the branch: `git push origin feature/my-feature`
5. Open a Pull Request

---

## 📄 License

MIT License — see [LICENSE](LICENSE) for details.

---

<div align="center">

Made with ❤️ for the Linux community

</div>
