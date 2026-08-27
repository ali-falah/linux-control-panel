import os
import re

base_dir = "src-tauri/src/commands"

for root, _, files in os.walk(base_dir):
    for file in files:
        if file.endswith(".rs") and file != "app_manager.rs":
            filepath = os.path.join(root, file)
            with open(filepath, "r") as f:
                content = f.read()

            content = re.sub(r"use tokio::process::Command;", "use crate::utils::privilege::tokio::Command;", content)
            content = re.sub(r"use std::process::Command;", "use crate::utils::privilege::Command;", content)

            with open(filepath, "w") as f:
                f.write(content)

# fix lib.rs
with open("src-tauri/src/lib.rs", "r") as f:
    content = f.read()
content = re.sub(r"tokio::process::Command::new", "crate::utils::privilege::tokio::Command::new", content)
with open("src-tauri/src/lib.rs", "w") as f:
    f.write(content)
