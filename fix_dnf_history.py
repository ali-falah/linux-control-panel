import re

with open("src-tauri/src/commands/dnf_history.rs", "r") as f:
    content = f.read()

# Replace the block from `let (is_none, pw_opt)` to `let app_clone = app.clone();`
old_block = r"""    let \(is_none, pw_opt\) = \{
        let guard = crate::utils::privilege::SUDO_PASSWORD.lock\(\)\.unwrap\(\);
        \(guard\.is_none\(\), if guard\.is_some\(\) \{ Some\(guard\.clone\(\)\.unwrap\(\)\) \} else \{ None \}\)
    \};
    if is_none \{
        return Err\("Root privileges are required to perform upgrades. Please enable Root in the Control Panel."\.to_string\(\)\);
    \}
    let pw = pw_opt\.unwrap\(\);

    let mut cmd = tokio::process::Command::new\("sudo"\);
    cmd\.arg\("-S"\)
       \.arg\("--prompt="\)
       \.arg\("python3"\)
       \.arg\("-c"\)
       \.arg\("import pty; import sys; pty\.spawn\(sys\.argv\[1:\]\)"\)
       \.arg\("dnf"\)
       \.arg\("upgrade"\)
       \.arg\("-y"\)
       \.args\(&packages\);
    
    cmd\.stdin\(Stdio::piped\(\)\)
       \.stdout\(Stdio::piped\(\)\)
       \.stderr\(Stdio::piped\(\)\);

    let mut child = cmd\.spawn\(\)\.map_err\(\|e\| e\.to_string\(\)\)\?;

    if let Some\(mut stdin\) = child\.stdin\.take\(\) \{
        let mut p = pw;
        p\.push\('\\n'\);
        tokio::spawn\(async move \{
            let _ = stdin\.write_all\(p\.as_bytes\(\)\)\.await;
        \}\);
    \}

    let mut stdout = child\.stdout\.take\(\)\.unwrap\(\);
    let mut stderr = child\.stderr\.take\(\)\.unwrap\(\);"""

new_block = """    let mut cmd = crate::utils::privilege::tokio::Command::new("pkexec");
    cmd.arg("python3")
       .arg("-c")
       .arg("import pty; import sys; pty.spawn(sys.argv[1:])")
       .arg("dnf")
       .arg("upgrade")
       .arg("-y")
       .args(&packages);
    
    cmd.stdout(Stdio::piped())
       .stderr(Stdio::piped());

    let mut child = cmd.spawn().map_err(|e| e.to_string())?;

    let mut stdout = child.stdout.take().unwrap();
    let mut stderr = child.stderr.take().unwrap();"""

# Because `tokio::process::Command` was replaced with `crate::utils::privilege::tokio::Command` in fix_commands.py, 
# the old block will actually have `crate::utils::privilege::tokio::Command::new("sudo")`.
old_block_real = old_block.replace("tokio::process::Command", "crate::utils::privilege::tokio::Command")

content = re.sub(old_block_real, new_block, content)

with open("src-tauri/src/commands/dnf_history.rs", "w") as f:
    f.write(content)
