use serde::{Deserialize, Serialize};
use reqwest::Client;
use std::time::Duration;
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiSettingsConfig {
    pub enabled: bool,           // default: true
    pub provider: String,        // "ollama" | "gemini" | "openai"
    pub ollama_url: String,      // default: "http://127.0.0.1:11434"
    pub ollama_model: String,    // default: "qwen2.5:1.5b"
    pub cloud_provider: String,  // "gemini" | "openai"
    pub api_key: String,
    pub cloud_model: String,     // default e.g. "gemini-2.5-flash" or "gpt-4o-mini"
}

impl Default for AiSettingsConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            provider: "ollama".to_string(),
            ollama_url: "http://127.0.0.1:11434".to_string(),
            ollama_model: "llama3.2:1b".to_string(),
            cloud_provider: "gemini".to_string(),
            api_key: String::new(),
            cloud_model: "gemini-2.5-flash".to_string(),
        }
    }
}

fn get_ai_settings_file_path() -> PathBuf {
    let mut dir = dirs::config_dir().unwrap_or_else(|| PathBuf::from("."));
    dir.push("linux-control-panel");
    let _ = fs::create_dir_all(&dir);
    dir.push("ai_settings.json");
    dir
}

#[tauri::command]
pub fn ai_load_settings() -> Result<AiSettingsConfig, String> {
    let path = get_ai_settings_file_path();
    if !path.exists() {
        return Ok(AiSettingsConfig::default());
    }
    let content = fs::read_to_string(&path).map_err(|e| e.to_string())?;
    let config: AiSettingsConfig = serde_json::from_str(&content).unwrap_or_default();
    Ok(config)
}

#[tauri::command]
pub fn ai_save_settings(settings: AiSettingsConfig) -> Result<(), String> {
    let path = get_ai_settings_file_path();
    let json = serde_json::to_string_pretty(&settings).map_err(|e| e.to_string())?;
    fs::write(&path, json).map_err(|e| format!("Failed to save AI settings to disk: {e}"))?;
    Ok(())
}

#[tauri::command]
pub fn open_system_config_file() -> Result<String, String> {
    let path = get_ai_settings_file_path();
    if !path.exists() {
        let default_config = AiSettingsConfig::default();
        if let Ok(json) = serde_json::to_string_pretty(&default_config) {
            let _ = fs::write(&path, json);
        }
    }
    std::process::Command::new("xdg-open")
        .arg(&path)
        .spawn()
        .map_err(|e| format!("Failed to launch system text editor: {}", e))?;
    Ok(path.to_string_lossy().to_string())
}

// ─── Data Types for AI Tasks ──────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityFindingInput {
    pub id: String,
    pub title: String,
    pub severity: String,
    pub category: String,
    pub description: String,
    pub current_value: String,
    pub recommendation: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiAdvisorResponse {
    pub risk_explanation: String,
    pub remediation_command: String,
    pub safety_notes: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogDiagnosisResponse {
    pub error_summary: String,
    pub root_cause: String,
    pub suggested_action: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DnfConflictResponse {
    pub conflict_summary: String,
    pub remediation_command: String,
    pub explanation: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NginxRuleResponse {
    pub generated_config: String,
    pub explanation: String,
    pub server_name: String,
    pub port: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FirewallRuleResponse {
    pub generated_command: String,
    pub rich_rule: String,
    pub explanation: String,
    pub zone: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OllamaModelInfo {
    pub name: String,
    pub size: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OllamaTagsResponse {
    pub models: Vec<OllamaModelInfo>,
}

#[tauri::command]
pub async fn ai_check_ollama_status(url: Option<String>) -> Result<Vec<String>, String> {
    let target_url = url.unwrap_or_else(|| "http://127.0.0.1:11434".to_string());
    let endpoint = format!("{}/api/tags", target_url.trim_end_matches('/'));

    let client = Client::builder()
        .timeout(Duration::from_secs(3))
        .build()
        .map_err(|e| e.to_string())?;

    let res = client
        .get(&endpoint)
        .send()
        .await
        .map_err(|e| format!("Ollama server not reachable at {target_url}: {e}"))?;

    let tags: OllamaTagsResponse = res
        .json()
        .await
        .map_err(|e| format!("Invalid response from Ollama: {e}"))?;

    let names: Vec<String> = tags.models.into_iter().map(|m| m.name).collect();
    Ok(names)
}

// ─── Core AI Request Dispatcher ──────────────────────────────────────────────

async fn dispatch_ai_request(
    system_prompt: &str,
    user_prompt: &str,
    settings_override: Option<AiSettingsConfig>,
) -> Result<String, String> {
    let settings = match settings_override {
        Some(s) => s,
        None => ai_load_settings().unwrap_or_default(),
    };

    if !settings.enabled {
        return Err("AI features are currently disabled in Settings.".to_string());
    }

    let client = Client::builder()
        .timeout(Duration::from_secs(60))
        .build()
        .map_err(|e| e.to_string())?;

    if settings.provider == "gemini" || (settings.provider == "cloud" && settings.cloud_provider == "gemini") {
        if settings.api_key.trim().is_empty() {
            return Err("Google Gemini API key is missing. Please enter your API key in Settings -> AI Engine.".to_string());
        }
        let model = if settings.cloud_model.trim().is_empty() {
            "gemini-2.5-flash".to_string()
        } else {
            settings.cloud_model.trim().to_string()
        };

        let url = format!(
            "https://generativelanguage.googleapis.com/v1beta/models/{}:generateContent?key={}",
            model, settings.api_key.trim()
        );

        let full_prompt = format!("{}\n\nUser Question:\n{}", system_prompt, user_prompt);
        let payload = serde_json::json!({
            "contents": [{
                "parts": [{ "text": full_prompt }]
            }],
            "generationConfig": {
                "responseMimeType": "application/json"
            }
        });

        let res = client
            .post(&url)
            .json(&payload)
            .send()
            .await
            .map_err(|e| format!("Gemini API request failed: {e}"))?;

        if !res.status().is_success() {
            let err_text = res.text().await.unwrap_or_default();
            return Err(format!("Gemini API error: {err_text}"));
        }

        let res_json: serde_json::Value = res
            .json()
            .await
            .map_err(|e| format!("Failed to parse Gemini JSON: {e}"))?;

        let text = res_json["candidates"][0]["content"]["parts"][0]["text"]
            .as_str()
            .ok_or_else(|| "Missing candidate response text in Gemini API output".to_string())?;

        return Ok(text.to_string());
    }

    if settings.provider == "openai" || (settings.provider == "cloud" && settings.cloud_provider == "openai") {
        if settings.api_key.trim().is_empty() {
            return Err("OpenAI API key is missing. Please enter your API key in Settings -> AI Engine.".to_string());
        }
        let model = if settings.cloud_model.trim().is_empty() {
            "gpt-4o-mini".to_string()
        } else {
            settings.cloud_model.trim().to_string()
        };

        let url = "https://api.openai.com/v1/chat/completions";
        let payload = serde_json::json!({
            "model": model,
            "messages": [
                { "role": "system", "content": system_prompt },
                { "role": "user", "content": user_prompt }
            ],
            "response_format": { "type": "json_object" }
        });

        let res = client
            .post(url)
            .header("Authorization", format!("Bearer {}", settings.api_key.trim()))
            .json(&payload)
            .send()
            .await
            .map_err(|e| format!("OpenAI API request failed: {e}"))?;

        if !res.status().is_success() {
            let err_text = res.text().await.unwrap_or_default();
            return Err(format!("OpenAI API error: {err_text}"));
        }

        let res_json: serde_json::Value = res
            .json()
            .await
            .map_err(|e| format!("Failed to parse OpenAI JSON: {e}"))?;

        let text = res_json["choices"][0]["message"]["content"]
            .as_str()
            .ok_or_else(|| "Missing content in OpenAI response".to_string())?;

        return Ok(text.to_string());
    }

    // Default: Local Ollama
    let ollama_url = if settings.ollama_url.trim().is_empty() {
        "http://127.0.0.1:11434".to_string()
    } else {
        settings.ollama_url.trim().to_string()
    };
    let endpoint = format!("{}/api/generate", ollama_url.trim_end_matches('/'));
    let model = if settings.ollama_model.trim().is_empty() {
        "qwen2.5:1.5b".to_string()
    } else {
        settings.ollama_model.trim().to_string()
    };

    let payload = serde_json::json!({
        "model": model,
        "prompt": format!("{}\n\n{}", system_prompt, user_prompt),
        "stream": false,
        "format": "json",
        "keep_alive": "30m",
        "options": {
            "num_thread": 8,
            "num_predict": 300,
            "num_ctx": 1536,
            "temperature": 0.2
        }
    });

    let res = client
        .post(&endpoint)
        .json(&payload)
        .send()
        .await
        .map_err(|e| format!("Ollama server request failed ({endpoint}): {e}"))?;

    if !res.status().is_success() {
        let err_text = res.text().await.unwrap_or_default();
        return Err(format!("Ollama API returned error: {err_text}"));
    }

    let res_json: serde_json::Value = res
        .json()
        .await
        .map_err(|e| format!("Failed to parse Ollama JSON: {e}"))?;

    let response_text = res_json["response"]
        .as_str()
        .ok_or_else(|| "Missing 'response' field in Ollama output".to_string())?;

    Ok(response_text.to_string())
}

fn sanitize_json_string(raw: &str) -> String {
    let mut result = String::with_capacity(raw.len() + 16);
    let mut in_string = false;
    let mut chars = raw.chars().peekable();

    while let Some(c) = chars.next() {
        if c == '"' {
            in_string = !in_string;
            result.push(c);
        } else if in_string && c == '\\' {
            if let Some(&next_c) = chars.peek() {
                match next_c {
                    '"' | '\\' | '/' | 'b' | 'f' | 'n' | 'r' | 't' => {
                        result.push('\\');
                        result.push(chars.next().unwrap());
                    }
                    'u' => {
                        result.push('\\');
                        result.push(chars.next().unwrap());
                    }
                    _ => {
                        // Invalid escape sequence in JSON string (e.g. \?, \s, \d, \.)
                        // Escape the backslash itself so it becomes a literal backslash \\ in JSON
                        result.push_str("\\\\");
                    }
                }
            } else {
                result.push_str("\\\\");
            }
        } else if in_string && (c == '\n' || c == '\r') {
            if c == '\n' {
                result.push_str("\\n");
            } else {
                result.push_str("\\r");
            }
        } else {
            result.push(c);
        }
    }
    result
}

fn extract_valid_json(raw: &str) -> String {
    let trimmed = raw.trim()
        .trim_start_matches("```json")
        .trim_start_matches("```")
        .trim_end_matches("```")
        .trim();

    let start = match trimmed.find('{') {
        Some(pos) => pos,
        None => return sanitize_json_string(trimmed),
    };

    let bytes = trimmed.as_bytes();
    let mut depth = 0;
    let mut end = start;
    let mut in_string = false;
    let mut escape = false;

    for i in start..bytes.len() {
        let b = bytes[i];
        if escape {
            escape = false;
            continue;
        }
        if b == b'\\' && in_string {
            escape = true;
            continue;
        }
        if b == b'"' {
            in_string = !in_string;
            continue;
        }
        if !in_string {
            if b == b'{' {
                depth += 1;
            } else if b == b'}' {
                depth -= 1;
                if depth == 0 {
                    end = i;
                    break;
                }
            }
        }
    }

    let json_substring = if depth == 0 && end >= start {
        &trimmed[start..=end]
    } else {
        trimmed
    };

    sanitize_json_string(json_substring)
}

#[tauri::command]
pub async fn ai_test_cloud_connection(
    provider: String,
    api_key: String,
    model: String,
) -> Result<String, String> {
    if api_key.trim().is_empty() {
        return Err("API key cannot be empty.".to_string());
    }

    let test_settings = AiSettingsConfig {
        enabled: true,
        provider: provider.clone(),
        ollama_url: String::new(),
        ollama_model: String::new(),
        cloud_provider: provider.clone(),
        api_key: api_key.trim().to_string(),
        cloud_model: if model.trim().is_empty() {
            if provider == "gemini" { "gemini-2.5-flash".to_string() } else { "gpt-4o-mini".to_string() }
        } else {
            model.trim().to_string()
        },
    };

    let system_prompt = "You are a fast API health checker.";
    let user_prompt = "Respond ONLY with valid JSON: {\"status\": \"ok\"}";

    dispatch_ai_request(system_prompt, user_prompt, Some(test_settings)).await?;
    Ok("Connection successful! API key and model verified.".to_string())
}

// ─── AI Task Implementations ─────────────────────────────────────────────────

#[tauri::command]
pub async fn ai_explain_security_finding(
    finding: SecurityFindingInput,
    settings: Option<AiSettingsConfig>,
) -> Result<AiAdvisorResponse, String> {
    let system_prompt = r#"You are an expert Fedora Linux Security Specialist. Analyze the provided security finding for a Fedora Linux system.

TARGET OS: Fedora Linux (systemd, DNF, firewalld, SELinux).

STRICT REMEDIATION COMMAND RULES FOR FEDORA LINUX:
1. EXCLUSIVELY LINUX COMMANDS: Never use BSD or macOS sysctl keys like "net.inet.ip.flood". All sysctl parameters MUST be valid Linux kernel parameters starting with net.ipv4, net.ipv6, kernel., or fs.
2. ACCURATE SSH CONFIG EDITS (/etc/ssh/sshd_config):
   - Directives are CaseSensitive (e.g. LoginGraceTime, PermitRootLogin, MaxAuthTries, PasswordAuthentication).
   - NEVER put a trailing slash on file paths (use "/etc/ssh/sshd_config", NOT "/etc/ssh/sshd_config/").
   - Use correct sed syntax to set the actual target value (e.g., `sudo sed -i 's/^#\?LoginGraceTime.*/LoginGraceTime 60/' /etc/ssh/sshd_config && sudo sshd -t && sudo systemctl reload sshd`).
3. EXECUTABLE SINGLE-LINE COMMAND: "remediation_command" MUST be a single, valid, safe bash shell command with sudo.
4. CRITICAL LANGUAGE REQUIREMENT: You MUST write your analysis strictly in clear, professional English. Do NOT output any Chinese or non-English characters.

Respond ONLY with a valid JSON object in the exact following structure without markdown formatting or backticks:
{
  "risk_explanation": "A concise 2-3 sentence explanation in English of why this finding is dangerous and how attackers exploit it.",
  "remediation_command": "The exact single-line bash command to fix it on Fedora Linux.",
  "safety_notes": "A short note in English on whether applying this fix could impact active services."
}"#;

    let user_prompt = format!(
        "Finding Title: {}\nSeverity: {}\nCategory: {}\nDescription: {}\nCurrent Value: {}\nRecommendation: {}\n\nProvide structured JSON response.",
        finding.title,
        finding.severity,
        finding.category,
        finding.description,
        finding.current_value,
        finding.recommendation
    );

    let raw_output = dispatch_ai_request(system_prompt, &user_prompt, settings).await?;
    let clean = extract_valid_json(&raw_output);
    let resp: AiAdvisorResponse = serde_json::from_str(&clean)
        .map_err(|e| format!("Failed to parse AI response: {e}\nRaw output: {clean}"))?;
    Ok(resp)
}

#[tauri::command]
pub async fn ai_diagnose_log_error(
    log_context: String,
    service_name: Option<String>,
    settings: Option<AiSettingsConfig>,
) -> Result<LogDiagnosisResponse, String> {
    let system_prompt = r#"You are an expert Fedora Linux Administrator and Log Analyst. Analyze the provided systemd journal log or audit error for a Fedora Linux system.

TARGET OS: Fedora Linux (systemd, DNF, firewalld, SELinux).

CRITICAL CONSTRAINTS:
1. Suggest strictly Linux/Fedora commands (systemctl, journalctl, dnf, firewall-cmd, sysctl).
2. Never suggest BSD/macOS commands or invalid sysctl keys.
3. CRITICAL LANGUAGE REQUIREMENT: You MUST write your diagnosis strictly in clear, professional English.

Respond ONLY with a valid JSON object in the exact following structure without markdown formatting or backticks:
{
  "error_summary": "A concise 1-2 sentence summary in English of what went wrong.",
  "root_cause": "The specific root cause (e.g. missing dependency, port conflict, permission denied, invalid configuration).",
  "suggested_action": "The recommended single-line terminal command or fix step to resolve the error on Fedora Linux."
}"#;

    let target_service = service_name.unwrap_or_else(|| "System Log / Audit".to_string());
    let user_prompt = format!("Service Context: {}\n\nLog Lines:\n{}", target_service, log_context);

    let raw_output = dispatch_ai_request(system_prompt, &user_prompt, settings).await?;
    let clean = extract_valid_json(&raw_output);
    let resp: LogDiagnosisResponse = serde_json::from_str(&clean)
        .map_err(|e| format!("Failed to parse AI diagnosis: {e}\nRaw output: {clean}"))?;
    Ok(resp)
}

#[tauri::command]
pub async fn ai_explain_dnf_conflict(
    terminal_output: String,
    settings: Option<AiSettingsConfig>,
) -> Result<DnfConflictResponse, String> {
    let system_prompt = r#"You are a Red Hat / Fedora DNF Package Manager Specialist. Analyze the provided DNF transaction failure log on Fedora Linux.

TARGET OS: Fedora Linux (DNF5/DNF, RPM, rpm-ostree).

CRITICAL CONSTRAINTS:
1. Provide exact DNF/RPM commands (e.g. `sudo dnf clean all`, `sudo dnf upgrade --allowerasing`, `sudo rpm --import <key>`).
2. CRITICAL LANGUAGE REQUIREMENT: You MUST write your explanation strictly in clear, professional English.

Respond ONLY with a valid JSON object in the exact following structure without markdown formatting or backticks:
{
  "conflict_summary": "A clear explanation of why DNF failed (e.g. broken GPG key, repository lock, package version conflict, or missing dependency).",
  "remediation_command": "The exact single-line shell command to fix it on Fedora Linux.",
  "explanation": "Brief additional context or advice for the user in English."
}"#;

    let user_prompt = format!("DNF Terminal Output:\n{}", terminal_output);

    let raw_output = dispatch_ai_request(system_prompt, &user_prompt, settings).await?;
    let clean = extract_valid_json(&raw_output);
    let resp: DnfConflictResponse = serde_json::from_str(&clean)
        .map_err(|e| format!("Failed to parse AI DNF conflict response: {e}\nRaw output: {clean}"))?;
    Ok(resp)
}

#[tauri::command]
pub async fn ai_generate_nginx_rule(
    prompt: String,
    settings: Option<AiSettingsConfig>,
) -> Result<NginxRuleResponse, String> {
    let system_prompt = r#"You are an expert NGINX Web Server Specialist for Fedora Linux. Generate a valid, production-ready NGINX server configuration block based on the user's natural language request.

TARGET OS: Fedora Linux (/etc/nginx/conf.d/, /etc/nginx/nginx.conf).

CRITICAL CONSTRAINTS:
1. Use standard NGINX directive syntax.
2. CRITICAL LANGUAGE REQUIREMENT: All explanations MUST be strictly in clear English.

Respond ONLY with a valid JSON object in the exact following structure without markdown formatting or backticks:
{
  "generated_config": "The complete NGINX server block configuration string.",
  "explanation": "A short summary in English of what this configuration does.",
  "server_name": "The extracted domain or server_name (e.g. example.com or localhost).",
  "port": 80
}"#;

    let user_prompt = format!("User Request:\n{}", prompt);

    let raw_output = dispatch_ai_request(system_prompt, &user_prompt, settings).await?;
    let clean = extract_valid_json(&raw_output);
    let resp: NginxRuleResponse = serde_json::from_str(&clean)
        .map_err(|e| format!("Failed to parse AI NGINX response: {e}\nRaw output: {clean}"))?;
    Ok(resp)
}

#[tauri::command]
pub async fn ai_generate_firewall_rule(
    prompt: String,
    settings: Option<AiSettingsConfig>,
) -> Result<FirewallRuleResponse, String> {
    let system_prompt = r#"You are a Fedora Linux Network & Firewalld Specialist. Generate a valid firewalld rule or rich rule based on the user's natural language request.

TARGET OS & FIREWALL: Fedora Linux with firewalld (firewall-cmd).

CRITICAL CONSTRAINTS:
1. Always use `firewall-cmd` syntax (e.g. `sudo firewall-cmd --permanent --add-port=8080/tcp && sudo firewall-cmd --reload`).
2. CRITICAL LANGUAGE REQUIREMENT: All explanations MUST be strictly in clear English.

Respond ONLY with a valid JSON object in the exact following structure without markdown formatting or backticks:
{
  "generated_command": "The exact firewall-cmd terminal command (e.g. sudo firewall-cmd --permanent --add-port=8080/tcp && sudo firewall-cmd --reload).",
  "rich_rule": "The rich rule string if applicable (or empty string).",
  "explanation": "A short explanation in English of what this rule does.",
  "zone": "The target firewalld zone (e.g. public)."
}"#;

    let user_prompt = format!("User Request:\n{}", prompt);

    let raw_output = dispatch_ai_request(system_prompt, &user_prompt, settings).await?;
    let clean = extract_valid_json(&raw_output);
    let resp: FirewallRuleResponse = serde_json::from_str(&clean)
        .map_err(|e| format!("Failed to parse AI Firewall response: {e}\nRaw output: {clean}"))?;
    Ok(resp)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TerminalCommandResponse {
    pub generated_command: String,
    pub explanation: String,
    pub safety_level: String, // "safe" | "caution" | "dangerous"
}

#[tauri::command]
pub async fn ai_generate_terminal_command(
    prompt: String,
    settings: Option<AiSettingsConfig>,
) -> Result<TerminalCommandResponse, String> {
    let system_prompt = r#"You are an expert Fedora Linux Terminal & Shell Specialist. Translate the user's natural language request into a valid, precise bash terminal command string for Fedora Linux.

TARGET OS: Fedora Linux.

CRITICAL CONSTRAINTS:
1. Provide single-line executable bash command strings.
2. Provide a clear 1-2 sentence explanation of what the command does.
3. Categorize safety_level as "safe" (read-only find/grep/ls), "caution" (modifies files/installs packages), or "dangerous" (destructive rm/dd/mkfs).
4. CRITICAL LANGUAGE REQUIREMENT: All explanations MUST be strictly in clear English.

Respond ONLY with a valid JSON object in the exact following structure without markdown formatting or backticks:
{
  "generated_command": "The exact bash command string.",
  "explanation": "A short summary in English of what this command does.",
  "safety_level": "safe"
}"#;

    let user_prompt = format!("User Request:\n{}", prompt);

    let raw_output = dispatch_ai_request(system_prompt, &user_prompt, settings).await?;
    let clean = extract_valid_json(&raw_output);
    let resp: TerminalCommandResponse = serde_json::from_str(&clean)
        .map_err(|e| format!("Failed to parse AI Terminal response: {e}\nRaw output: {clean}"))?;
    Ok(resp)
}
