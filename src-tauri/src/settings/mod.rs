use anyhow::Result;
use serde::{Deserialize, Serialize};
use sqlx::{Row, SqlitePool};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    pub groq_key: String,
    pub kb_folder: String,
    pub nudge_enabled: bool,
    pub ai_suggestions_enabled: bool,
    pub nudge_interval_secs: u32,
    pub nudge_threshold: f32,
    pub whisper_model: String,
    pub screen_share_protection: bool,
    pub auto_start: bool,
    pub obsidian_vault: String,
    pub webhook_url: String,
    pub parakeet_enabled: bool,
    pub mcp_enabled: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            groq_key: String::new(),
            kb_folder: String::new(),
            nudge_enabled: true,
            ai_suggestions_enabled: true,
            nudge_interval_secs: 5,
            nudge_threshold: 0.65,
            whisper_model: "whisper-base".into(),
            screen_share_protection: true,
            auto_start: false,
            obsidian_vault: String::new(),
            webhook_url: String::new(),
            parakeet_enabled: false,
            mcp_enabled: false,
        }
    }
}

pub async fn get(pool: &SqlitePool) -> Result<Settings> {
    let rows = sqlx::query("SELECT key, value FROM settings")
        .fetch_all(pool)
        .await?;

    let mut s = Settings::default();
    for row in rows {
        let key: String = row.get("key");
        let value: String = row.get("value");
        match key.as_str() {
            "groq_key" => s.groq_key = value,
            "kb_folder" => s.kb_folder = value,
            "nudge_enabled" => s.nudge_enabled = value == "true",
            "ai_suggestions_enabled" => s.ai_suggestions_enabled = value == "true",
            "nudge_interval_secs" => s.nudge_interval_secs = value.parse().unwrap_or(5),
            "nudge_threshold" => s.nudge_threshold = value.parse().unwrap_or(0.65),
            "whisper_model" => s.whisper_model = value,
            "screen_share_protection" => s.screen_share_protection = value == "true",
            "auto_start" => s.auto_start = value == "true",
            "obsidian_vault" => s.obsidian_vault = value,
            "webhook_url" => s.webhook_url = value,
            "parakeet_enabled" => s.parakeet_enabled = value == "true",
            "mcp_enabled" => s.mcp_enabled = value == "true",
            _ => {}
        }
    }
    Ok(s)
}

pub async fn set(pool: &SqlitePool, key: &str, value: &str) -> Result<()> {
    sqlx::query(
        "INSERT INTO settings(key, value) VALUES(?, ?) ON CONFLICT(key) DO UPDATE SET value=excluded.value",
    )
    .bind(key)
    .bind(value)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn test_groq(key: &str) -> Result<bool> {
    if key.is_empty() {
        return Ok(false);
    }
    let client = reqwest::Client::new();
    let resp = client
        .get("https://api.groq.com/openai/v1/models")
        .header("Authorization", format!("Bearer {}", key))
        .timeout(std::time::Duration::from_secs(10))
        .send()
        .await?;
    Ok(resp.status().is_success())
}
