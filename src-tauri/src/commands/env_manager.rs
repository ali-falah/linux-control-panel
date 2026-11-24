use crate::utils::privilege::{run_privileged_with_stdin, RootAuth};
use crate::utils::root_policy::{validate_environment_content, ENVIRONMENT_PATH};
use serde::{Deserialize, Serialize};

/// One line of `/etc/environment`.
///
/// The file is a small, hand-edited document: it can contain comments and
/// blank lines next to the variables. The document is kept line-for-line so a
/// save never silently deletes comments, blank lines or lines the panel does
/// not understand (for example an `export`-style line another tool added).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnvLine {
    /// `entry` (editable variable), `comment`, `blank`, or `raw` (preserved
    /// verbatim because the panel does not understand it).
    pub kind: String,
    /// The exact source line for `comment`, `blank` and `raw`.
    pub text: String,
    #[serde(default)]
    pub key: String,
    #[serde(default)]
    pub value: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnvFile {
    pub path: String,
    pub lines: Vec<EnvLine>,
}

const ENV_PATH: &str = ENVIRONMENT_PATH;
const MAX_LINES: usize = 10_000;
const MAX_BYTES: usize = 256 * 1024;

fn is_valid_key(key: &str) -> bool {
    let mut chars = key.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if !first.is_ascii_alphabetic() && first != '_' {
        return false;
    }
    chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// Undo the quoting the file may use around a value. `pam_env` treats a
/// quoted value as one literal value, with `\"` and `\\` escaped inside
/// double quotes; single quotes are literal.
fn unquote(raw: &str) -> String {
    let value = raw.trim();
    if value.len() >= 2 && value.starts_with('"') && value.ends_with('"') {
        let inner = &value[1..value.len() - 1];
        let mut out = String::with_capacity(inner.len());
        let mut chars = inner.chars();
        while let Some(c) = chars.next() {
            if c == '\\' {
                match chars.next() {
                    Some(next @ ('"' | '\\')) => out.push(next),
                    Some(next) => {
                        out.push('\\');
                        out.push(next);
                    }
                    None => out.push('\\'),
                }
            } else {
                out.push(c);
            }
        }
        out
    } else if value.len() >= 2 && value.starts_with('\'') && value.ends_with('\'') {
        value[1..value.len() - 1].to_string()
    } else {
        value.to_string()
    }
}

fn quote(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 2);
    out.push('"');
    for c in value.chars() {
        if c == '\\' || c == '"' {
            out.push('\\');
        }
        out.push(c);
    }
    out.push('"');
    out
}

fn parse_line(line: &str) -> EnvLine {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return line_kind("blank", line.to_string());
    }
    if trimmed.starts_with('#') {
        return line_kind("comment", line.to_string());
    }

    // A variable line is `NAME=value` with the name starting at the first
    // non-space character. Anything else (`export NAME=value`, a stray word)
    // is preserved verbatim instead of being guessed at.
    if let Some((name, value)) = trimmed.split_once('=') {
        if name == name.trim() && !name.contains(char::is_whitespace) && is_valid_key(name) {
            return EnvLine {
                kind: String::from("entry"),
                text: line.to_string(),
                key: name.to_string(),
                value: unquote(value),
            };
        }
    }

    line_kind("raw", line.to_string())
}

fn line_kind(kind: &str, text: String) -> EnvLine {
    EnvLine {
        kind: kind.to_string(),
        text,
        key: String::new(),
        value: String::new(),
    }
}

fn parse_file(content: &str) -> Vec<EnvLine> {
    content.lines().map(parse_line).collect()
}

fn has_control_characters(value: &str) -> bool {
    value.contains(['\n', '\r', '\0'])
}

/// Validate the document before it is serialized. User input reaches this
/// only through the app, but a malformed key or an embedded newline would
/// corrupt a root-owned file, so the rules are enforced here as well.
fn validate(lines: &[EnvLine]) -> Result<(), String> {
    if lines.len() > MAX_LINES {
        return Err(format!(
            "The file has too many lines (maximum {MAX_LINES})."
        ));
    }

    let mut keys = std::collections::HashSet::new();
    let mut bytes = 0usize;

    for (index, item) in lines.iter().enumerate() {
        let number = index + 1;
        match item.kind.as_str() {
            "entry" => {
                if !is_valid_key(&item.key) {
                    return Err(format!(
                        "Line {number}: \"{}\" is not a valid variable name. Use letters, digits and underscores, starting with a letter or underscore.",
                        item.key
                    ));
                }
                if has_control_characters(&item.value) {
                    return Err(format!(
                        "Line {number}: a variable value must be a single line."
                    ));
                }
                if !keys.insert(item.key.clone()) {
                    return Err(format!(
                        "Line {number}: \"{}\" is defined more than once.",
                        item.key
                    ));
                }
                bytes += item.key.len() + item.value.len() + 4;
            }
            "comment" | "blank" | "raw" => {
                if has_control_characters(&item.text) {
                    return Err(format!(
                        "Line {number}: the line contains a control character."
                    ));
                }
                bytes += item.text.len() + 1;
            }
            other => {
                return Err(format!("Line {number}: unknown line type \"{other}\"."));
            }
        }
        if bytes > MAX_BYTES {
            return Err(String::from("The file is too large to write."));
        }
    }

    Ok(())
}

fn serialize(lines: &[EnvLine]) -> String {
    let mut out = String::new();
    for item in lines {
        match item.kind.as_str() {
            "entry" => {
                out.push_str(&item.key);
                out.push('=');
                out.push_str(&quote(&item.value));
                out.push('\n');
            }
            _ => {
                out.push_str(&item.text);
                out.push('\n');
            }
        }
    }
    out
}

#[tauri::command]
pub async fn read_env_file() -> Result<EnvFile, String> {
    let content = match std::fs::read_to_string(ENV_PATH) {
        Ok(content) => content,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(error) => {
            return Err(format!(
                "{ENV_PATH} could not be read: {error}. Administrator rights may be required."
            ))
        }
    };

    Ok(EnvFile {
        path: ENV_PATH.to_string(),
        lines: parse_file(&content),
    })
}

#[tauri::command]
pub async fn write_env_file(lines: Vec<EnvLine>) -> Result<String, String> {
    validate(&lines)?;
    let content = serialize(&lines);
    validate_environment_content(&content)?;

    let current = std::fs::read_to_string(ENV_PATH).unwrap_or_default();
    if current == content {
        return Ok(String::from("No changes to write"));
    }

    // The general script channel is fail-closed by the privilege layer, so
    // this one fixed file has its own validated helper action. The helper
    // re-checks the content and keeps a revision backup.
    let helper = crate::commands::admin::installed_helper_path().ok_or_else(|| {
        "The administrator helper is not installed; /etc/environment was not written.".to_string()
    })?;
    let args = vec![String::from("write_environment_file")];
    let output = run_privileged_with_stdin(helper, &args, &RootAuth::Polkit, &content).await?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(if stderr.is_empty() {
            String::from("The administrator helper did not write /etc/environment.")
        } else {
            stderr
        });
    }

    Ok(format!("Saved {ENV_PATH}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entries(lines: &[EnvLine]) -> Vec<(&str, &str)> {
        lines
            .iter()
            .filter(|line| line.kind == "entry")
            .map(|line| (line.key.as_str(), line.value.as_str()))
            .collect()
    }

    #[test]
    fn parses_quoted_values_and_preserves_comments() {
        let content = "\
# System-wide environment variables
# Managed by the administrator
PATH=\"/usr/local/sbin:/usr/bin\"
JAVA_HOME=/usr/lib/jvm/java-21
GREETING=\"hello world\"
SINGLE='literal $HOME'
EMPTY=

export LEGACY=1
BROKEN LINE
";
        let lines = parse_file(content);

        assert_eq!(
            entries(&lines),
            vec![
                ("PATH", "/usr/local/sbin:/usr/bin"),
                ("JAVA_HOME", "/usr/lib/jvm/java-21"),
                ("GREETING", "hello world"),
                ("SINGLE", "literal $HOME"),
                ("EMPTY", ""),
            ]
        );
        assert_eq!(lines.iter().filter(|l| l.kind == "comment").count(), 2);
        assert_eq!(lines.iter().filter(|l| l.kind == "raw").count(), 2);
        assert!(lines
            .iter()
            .any(|l| l.kind == "raw" && l.text == "export LEGACY=1"));
    }

    #[test]
    fn round_trips_escaped_values() {
        let content = "ESCAPED=\"a \\\"quote\\\" and a \\\\ slash\"\n";
        let lines = parse_file(content);
        assert_eq!(
            entries(&lines),
            vec![("ESCAPED", "a \"quote\" and a \\ slash")]
        );
        assert_eq!(serialize(&lines), content);
    }

    #[test]
    fn preserves_exact_comment_and_blank_lines() {
        let content = "# keep me\n\n   # indented comment\nFOO=bar\n";
        let lines = parse_file(content);
        // Comments and blank lines keep their exact bytes; only the variable
        // line is normalised when it is written back.
        assert_eq!(
            serialize(&lines),
            "# keep me\n\n   # indented comment\nFOO=\"bar\"\n"
        );
    }

    #[test]
    fn writing_normalises_only_variable_lines() {
        let lines = vec![
            line_kind("comment", String::from("# header")),
            EnvLine {
                kind: String::from("entry"),
                text: String::from("FOO='old'"),
                key: String::from("FOO"),
                value: String::from("new value"),
            },
            line_kind("blank", String::new()),
        ];
        assert_eq!(serialize(&lines), "# header\nFOO=\"new value\"\n\n");
    }

    #[test]
    fn rejects_invalid_and_duplicate_keys() {
        let invalid = vec![EnvLine {
            kind: String::from("entry"),
            text: String::new(),
            key: String::from("1BAD"),
            value: String::new(),
        }];
        assert!(validate(&invalid).unwrap_err().contains("not a valid"));

        let duplicate = vec![
            EnvLine {
                kind: String::from("entry"),
                text: String::new(),
                key: String::from("FOO"),
                value: String::from("a"),
            },
            EnvLine {
                kind: String::from("entry"),
                text: String::new(),
                key: String::from("FOO"),
                value: String::from("b"),
            },
        ];
        assert!(validate(&duplicate).unwrap_err().contains("more than once"));
    }

    #[test]
    fn rejects_multiline_values_and_preserved_lines() {
        for item in [
            EnvLine {
                kind: String::from("entry"),
                text: String::new(),
                key: String::from("FOO"),
                value: String::from("a\nb"),
            },
            line_kind("raw", String::from("one\ntwo")),
            line_kind("comment", String::from("a\rb")),
        ] {
            assert!(validate(&[item]).is_err());
        }
    }

    #[test]
    fn value_quoting_round_trips_backslashes_and_quotes() {
        let original = r#"C:\tools\"quoted\""#;
        let quoted = format!("VALUE={}", quote(original));
        let lines = parse_file(&quoted);
        assert_eq!(entries(&lines), vec![("VALUE", original)]);
    }

    #[test]
    fn serialized_documents_pass_the_helper_policy() {
        for content in [
            "",
            "# header\nPATH=\"/usr/bin\"\n",
            "export LEGACY=1\nEDITOR=\"vim\"\n",
            "A=\"one\"\nB=\"two with spaces\"\n",
        ] {
            assert!(
                validate_environment_content(content).is_ok(),
                "{content:?} must pass the helper policy"
            );
        }

        let parsed = parse_file("# keep\nexport LEGACY=1\nFOO=\"bar\"\n");
        assert!(validate_environment_content(&serialize(&parsed)).is_ok());
    }

    #[test]
    fn empty_file_is_fine() {
        assert!(parse_file("").is_empty());
        assert!(validate(&[]).is_ok());
    }
}
