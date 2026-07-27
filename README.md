<div align="center">

# 🖥️ Linux Control Panel

**A modern, enterprise-grade system management desktop application for Fedora, RHEL, and Linux distributions.**

[![Tauri](https://img.shields.io/badge/Tauri-v2.x-24C8D8?style=for-the-badge&logo=tauri&logoColor=white)](https://tauri.app)
[![Svelte](https://img.shields.io/badge/Svelte-v5.x-FF3E00?style=for-the-badge&logo=svelte&logoColor=white)](https://svelte.dev)
[![Rust](https://img.shields.io/badge/Rust-1.77+-CE422B?style=for-the-badge&logo=rust&logoColor=white)](https://www.rust-lang.org)
[![Fedora](https://img.shields.io/badge/Fedora-40%2B-294172?style=for-the-badge&logo=fedora&logoColor=white)](https://fedoraproject.org)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg?style=for-the-badge)](LICENSE)

> A blazing-fast, native desktop application engineered to manage system telemetry, security hardening, systemd services, packages, firewall, network connections, Nginx server blocks, SELinux, and system configurations — all from a stunning modern UI.

</div>

---

## 🌟 Highlights & Architecture

- ⚡ **Tauri v2 & Svelte 5**: Native Rust performance with ultra-fast Svelte 5 fine-grained reactivity (`$state`, `$derived`, `$effect`).
- 🚀 **Selective Dynamic Loading**: Heavy modules (`NginxManager`, `SecurityAuditor`, `ShellEnv`) are lazy-loaded on demand, keeping initial bundle size (~900KB) and RAM footprint minimal.
- 🕒 **Split Back Button & History Dropdown (`< Back ▾ 🕒`)**: Native browser-like navigation with a unified split Back button and floating dropdown menu for recently visited pages, eliminating sidebar layout shifting.
- 📈 **Persistent Telemetry History**: Thread-safe 60-second rolling history buffer in Rust memory (`TELEMETRY_HISTORY`) ensuring CPU, RAM, and network telemetry graphs hydrate instantly across navigation.
- 🔐 **PolicyKit Privilege Escalation**: Seamless graphical authentication via `pkexec` with structured PolicyKit error handling — **no `sudo` required**.
- 🛠️ **Strongly-Typed Rust Backend**: Centralized `AppError` enum using `thiserror` for clean, reliable IPC error handling.

---

## 🧰 20 Comprehensive Functional Modules

### 📊 Overview & Telemetry
| Module | Description & Capabilities |
| :--- | :--- |
| 📊 **System Dashboard** | Real-time Hardware & OS specifications (kernel, CPU, GPU, host), active network interfaces summary with type badges, interactive **BTRFS Storage Distribution & Subvolume Trees** (`├─`, `└─`), security audit quick rescan, and event button tooltips. |
| 📈 **System Monitor** | Live CPU utilization sparklines with **Expandable Per-Core Grid (`C0`..`C7`)**, RAM/Swap utilization progress bars, persistent 60s telemetry buffer, **Top Resource Consumers Mini-Panel** (Top 3 CPU & Top 3 RAM processes), **Active Connections Inspector** (filter by `All`, `Listen`, `Estab`, `External` with live search & inline process termination), and process tree manager. |

### 🛡️ Security & Logs
| Module | Description & Capabilities |
| :--- | :--- |
| 🛡️ **Security Auditor** | Automated system hardening audit based on CIS/Fedora benchmarks. Calculates compliance score (% score), categorizes findings (Kernel, Network, User Privileges, System Integrity, Firewall), provides interactive fix guides and 1-click remediation. Exports audit reports in **PDF**, **Styled HTML**, and **JSON** with **Open Downloads Folder** integration. |
| 📜 **Journal Logs & Threats** | Real-time `journalctl` log stream with log level priority filtering (`Error`, `Warning`, `Info`, `Debug`), **Auth Events Tab** (SSH logins, sudo escalations, failed password attempts), **Command Audit Tab** (terminal command auditing), **Threat Detection Tab** (brute-force & unauthorized access analysis), and custom Date/Time range picker. |
| 🛡️ **SELinux Manager** | SELinux Mode Switcher (`Enforcing`, `Permissive`, `Disabled`) with configuration persistence (`/etc/selinux/config`), SELinux Booleans Manager (search, view descriptions, toggle booleans with `setsebool -P`), and audit log denial parser (`ausearch`/`sealert`). |

### 🌐 Server & Web
| Module | Description & Capabilities |
| :--- | :--- |
| 🌐 **Nginx Manager** | Nginx virtual host scanner & server block inspector (domains, ports, SSL, document roots), integrated CodeMirror configuration editor, syntax testing (`nginx -t`), live reload (`nginx -s reload`), `access.log` & `error.log` live tail viewers, and Certbot SSL helper. |
| 🌐 **Advanced Network** | NetworkManager connection inspector (Wi-Fi, Ethernet, VPN, Bridge, Loopback), IP v4/v6, MAC Address, MTU, Gateway, DNS configuration, **Wi-Fi Access Point Scanner** (SSID, Signal Strength dBm/%, WPA2/WPA3 security, connection wizard), and Gateway Ping Latency Tester. |
| 🔥 **Firewall Manager** | Firewalld zone manager (`public`, `work`, `home`, `internal`, `trusted`), TCP/UDP port rule management, service toggles (`http`, `https`, `ssh`, `dns`, `ftp`, `wireguard`), Rich Rules editor, Interface-to-Zone mapping, and **Panic Mode Emergency Toggle** (instantly block all network traffic). |

### 📦 Package & Application Management
| Module | Description & Capabilities |
| :--- | :--- |
| 📦 **App Manager** | Unified package management across **RPM**, **DNF**, **Flatpak**, and **AppImage**. Category filtering, package search, update checks, package dependency tree inspector, duplicate package detector (Flatpak vs RPM instances), and local AppImage scanner & desktop registration. |
| 🗄️ **Repo Manager** | DNF Repository configuration inspector (`/etc/yum.repos.d/*.repo`), 1-click enable/disable toggles, add custom DNF repositories (`baseurl`, `gpgkey`), GPG key verification, and repository cache refreshing (`dnf makecache`). |
| 📜 **DNF History** | Complete DNF package manager transaction history log (`dnf history`), detailed package modification inspector (packages installed, upgraded, or removed), and 1-Click Transaction Rollback (`dnf history undo`). |
| 🔍 **Copr Browser** | Search community COPR repositories on Fedora COPR, view package listings & author information, and enable/disable COPR repositories (`dnf copr enable`). |

### ⚙️ System & Administration
| Module | Description & Capabilities |
| :--- | :--- |
| ⚙️ **Service Manager** | Systemd Unit Browser (Services, Timers, Sockets, Mounts, Targets), status filtering (`Active`, `Failed`, `Enabled`, `Disabled`), lifecycle controls (Start, Stop, Restart, Enable, Disable, Mask, Unmask), unit file inspector, and inline `journalctl -u` log stream. |
| 🔌 **Device Manager** | PCI & USB Device Explorer (`lspci`, `lsusb`), Block Storage Devices (`lsblk`) with filesystem types & mount points, CPU/GPU hardware specs, Thermal Sensors inspector (core temps, fan speeds), and **SMART Disk Health & Self-Test Inspector** (`smartctl`). |
| 🚀 **GRUB Bootloader** | GRUB configuration manager (`/etc/default/grub`), Kernel Command Line Arguments editor (`GRUB_CMDLINE_LINUX`), default boot entry selector, boot timeout adjustment, and GRUB config rebuild trigger (`grub2-mkconfig`). |
| 👥 **User Manager** | System Users & Groups manager (`/etc/passwd`, `/etc/group`), user creation & deletion, password management, UID/GID config, default shell selector (`bash`, `zsh`, `fish`), **Sudoers Privilege Rule Builder** (`/etc/sudoers.d/`), and SSH Authorized Keys manager (`~/.ssh/authorized_keys`). |
| 💻 **Shell Environment** | Graphical editor for shell environment files (`.bashrc`, `.zshrc`, `.bash_profile`), **Shell Aliases Manager** (create, edit, toggle, delete command aliases), **PATH Variable Visual Manager** (add, reorder, or remove directory paths), and shell config backup helper. |
| 🌐 **Hosts Manager** | Graphical `/etc/hosts` domain mapping editor, IP-to-Domain entry management with status toggles (enable/disable without deletion), pre-configured adware/malware blocklist integration, and domain ping verification tool. |
| ⏱️ **Cron Manager** | User Crontab & System Cron jobs inspector (`/etc/crontab`, `/etc/cron.*`), Visual Cron Expression Builder (Minute, Hour, Day, Month, Day of Week), syntax helper, job enable/disable toggle, and execution log stream. |
| ⚙️ **Env Manager** | Global system environment variables manager (`/etc/environment`), PAM environment inspector (`~/.pam_environment`), create, edit, and remove system-wide environment variables. |

---

## 🛠️ Tech Stack

| Layer | Technology |
|:---|:---|
| **Backend** | Rust, Tauri v2, Tokio Async Process Management |
| **Frontend** | Svelte 5 (Runes `$state`/`$derived`), Vanilla CSS Design System |
| **Editor** | CodeMirror 6 |
| **Icons** | Lucide Svelte |
| **Privilege Escalation** | PolicyKit (`pkexec`) |
| **Packaging** | RPM, AppImage |

---

## 📋 Prerequisites

### System Dependencies (Fedora 40+ / RHEL 9+)

```bash
# Rust Toolchain
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source $HOME/.cargo/env

# Tauri & WebKit System Dependencies
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

---

## 🚀 Installation & Build

```bash
# Clone the repository
git clone https://github.com/your-org/linux-control-panel
cd linux-control-panel

# Install frontend dependencies
npm install

# Run in Development Mode (Hot Reload)
npm run tauri:dev

# Production Build (generates RPM & AppImage bundles)
npm run tauri:build
```

Build outputs will be generated in `src-tauri/target/release/bundle/`:
- `rpm/linux-control-panel-*.x86_64.rpm`
- `appimage/linux-control-panel_*.AppImage`

### Install RPM Package

```bash
sudo dnf install ./src-tauri/target/release/bundle/rpm/linux-control-panel-*.x86_64.rpm
```

---

## 🔐 PolicyKit Configuration

The application uses PolicyKit for graphical privilege escalation — **no `sudo` required**.

To allow privileged operations (modifying system files, managing services, package management), install the Polkit policy:

```bash
sudo install -m 644 \
  src-tauri/polkit/com.controlpanel.pkexec.policy \
  /usr/share/polkit-1/actions/
```

---

## 🧑‍💻 Development & Quality Assurance

```bash
# Check Rust backend compilation
cd src-tauri && cargo check

# Run Rust unit tests
cd src-tauri && cargo test

# Build frontend production bundle
npm run build
```

---

## 📄 License

MIT License — see [LICENSE](LICENSE) for details.

---

<div align="center">


</div>
