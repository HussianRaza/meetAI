use anyhow::{anyhow, Result};
use reqwest::Client;

const GROQ_URL: &str = "https://api.groq.com/openai/v1/chat/completions";
pub const MODEL_LIVE: &str = "llama-3.1-8b-instant";

/// Single (non-streaming) Groq chat call. Returns the assistant reply text.
pub async fn chat(
    client: &Client,
    groq_key: &str,
    model: &str,
    messages: Vec<serde_json::Value>,
    max_tokens: u32,
) -> Result<String> {
    if groq_key.is_empty() {
        return Err(anyhow!("Groq API key not configured"));
    }

    let body = serde_json::json!({
        "model": model,
        "messages": messages,
        "max_tokens": max_tokens,
        "temperature": 0.7,
        "stream": false,
    });

    let resp = client
        .post(GROQ_URL)
        .header("Authorization", format!("Bearer {}", groq_key))
        .header("Content-Type", "application/json")
        .json(&body)
        .timeout(std::time::Duration::from_secs(15))
        .send()
        .await?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(anyhow!("Groq API {status}: {body}"));
    }

    let json: serde_json::Value = resp.json().await?;
    let text = json
        .pointer("/choices/0/message/content")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow!("unexpected Groq response: {json}"))?
        .trim()
        .to_string();

    Ok(text)
}
