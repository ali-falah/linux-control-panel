use serde::Serialize;
use thiserror::Error;

#[derive(Debug, Error, Serialize)]
#[serde(tag = "kind", content = "message")]
pub enum AppError {
    #[error("Permission denied: {0}")]
    PermissionDenied(String),

    #[error("PolicyKit authentication canceled or denied: {0}")]
    PolicyKitDenied(String),

    #[error("Not found: {0}")]
    NotFound(String),

    #[error("Command failed: {0}")]
    CommandFailed(String),

    #[error("IO error: {0}")]
    Io(String),

    #[error("Parse error: {0}")]
    Parse(String),
}

impl AppError {
    /// Checks stderr/stdout for PolicyKit or permission denial patterns
    pub fn from_process_output(stderr: &str, stdout: &str) -> Self {
        let combined = format!("{stderr} {stdout}").trim().to_string();
        let lower = combined.to_lowercase();
        if lower.contains("not authorized")
            || lower.contains("polkit-error-quark")
            || lower.contains("dismissed")
            || lower.contains("canceled")
            || lower.contains("cancelled")
            || lower.contains("authentication failed")
            || lower.contains("action matches no rule")
        {
            AppError::PolicyKitDenied(
                "Administrative privileges required: PolicyKit authorization was canceled or denied."
                    .to_string(),
            )
        } else if lower.contains("permission denied") || lower.contains("operation not permitted") {
            AppError::PermissionDenied(if combined.is_empty() {
                "Operation not permitted. Administrative privileges required.".to_string()
            } else {
                combined
            })
        } else {
            AppError::CommandFailed(if combined.is_empty() {
                "Command failed with non-zero exit code.".to_string()
            } else {
                combined
            })
        }
    }
}

impl From<std::io::Error> for AppError {
    fn from(err: std::io::Error) -> Self {
        AppError::Io(err.to_string())
    }
}
