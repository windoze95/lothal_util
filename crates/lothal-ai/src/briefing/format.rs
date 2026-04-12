use crate::AiError;

/// Output target for daily briefings.
pub enum BriefingOutput {
    Stdout,
    HomeAssistant { base_url: String, token: String },
    Slack { webhook_url: String },
}

impl BriefingOutput {
    /// Build from environment variables.
    ///
    /// Reads `BRIEFING_OUTPUT` ("stdout", "ha", or "slack") and the
    /// target-specific vars.
    pub fn from_env() -> Result<Self, AiError> {
        let target = std::env::var("BRIEFING_OUTPUT").unwrap_or_else(|_| "stdout".into());

        match target.to_lowercase().as_str() {
            "stdout" => Ok(Self::Stdout),
            "ha" | "homeassistant" | "home_assistant" => {
                let base_url = std::env::var("HA_BASE_URL").map_err(|_| {
                    AiError::ProviderNotConfigured("HA_BASE_URL required for HA output".into())
                })?;
                let token = std::env::var("HA_TOKEN").map_err(|_| {
                    AiError::ProviderNotConfigured("HA_TOKEN required for HA output".into())
                })?;
                Ok(Self::HomeAssistant { base_url, token })
            }
            "slack" => {
                let webhook_url = std::env::var("SLACK_WEBHOOK_URL").map_err(|_| {
                    AiError::ProviderNotConfigured(
                        "SLACK_WEBHOOK_URL required for Slack output".into(),
                    )
                })?;
                Ok(Self::Slack { webhook_url })
            }
            other => Err(AiError::ProviderNotConfigured(format!(
                "Unknown briefing output '{other}'. Use 'stdout', 'ha', or 'slack'."
            ))),
        }
    }

    /// Send a briefing to the configured output target.
    pub async fn send(&self, content: &str) -> Result<(), AiError> {
        match self {
            Self::Stdout => {
                println!("{content}");
                Ok(())
            }
            Self::HomeAssistant { base_url, token } => {
                send_to_ha(base_url, token, content).await
            }
            Self::Slack { webhook_url } => send_to_slack(webhook_url, content).await,
        }
    }
}

async fn send_to_ha(base_url: &str, token: &str, content: &str) -> Result<(), AiError> {
    let client = reqwest::Client::new();
    let url = format!("{base_url}/api/services/notify/notify");

    let body = serde_json::json!({
        "message": content,
        "title": "Lothal Daily Briefing",
    });

    let resp = client
        .post(&url)
        .header("Authorization", format!("Bearer {token}"))
        .header("Content-Type", "application/json")
        .json(&body)
        .send()
        .await?;

    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        return Err(AiError::LlmRequest(format!(
            "Home Assistant notification failed {status}: {text}"
        )));
    }

    Ok(())
}

async fn send_to_slack(webhook_url: &str, content: &str) -> Result<(), AiError> {
    let client = reqwest::Client::new();

    let body = serde_json::json!({
        "text": content,
    });

    let resp = client.post(webhook_url).json(&body).send().await?;

    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        return Err(AiError::LlmRequest(format!(
            "Slack webhook failed {status}: {text}"
        )));
    }

    Ok(())
}
