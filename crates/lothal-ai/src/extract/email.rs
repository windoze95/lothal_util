use sqlx::PgPool;
use uuid::Uuid;

use crate::provider::LlmClient;
use crate::AiError;

/// Configuration for IMAP email polling.
#[derive(Clone)]
pub struct ImapConfig {
    pub server: String,
    pub port: u16,
    pub username: String,
    pub password: String,
    pub folder: String,
    pub processed_folder: String,
    pub sender_patterns: Vec<String>,
}

impl ImapConfig {
    pub fn from_env() -> Result<Self, AiError> {
        let server = required_env("IMAP_SERVER")?;
        let port = std::env::var("IMAP_PORT")
            .unwrap_or_else(|_| "993".into())
            .parse()
            .unwrap_or(993);
        let username = required_env("IMAP_USER")?;
        let password = required_env("IMAP_PASSWORD")?;
        let folder = std::env::var("IMAP_FOLDER").unwrap_or_else(|_| "INBOX".into());
        let processed_folder =
            std::env::var("IMAP_PROCESSED_FOLDER").unwrap_or_else(|_| "Lothal/Processed".into());

        let patterns = std::env::var("IMAP_SENDER_PATTERNS")
            .unwrap_or_else(|_| "oge,ong,guthrie,oklahoma gas,oklahoma natural".into());
        let sender_patterns = patterns
            .split(',')
            .map(|s| s.trim().to_lowercase())
            .collect();

        Ok(Self {
            server,
            port,
            username,
            password,
            folder,
            processed_folder,
            sender_patterns,
        })
    }
}

/// Result of processing a single email.
#[derive(Debug)]
pub struct EmailIngestResult {
    pub message_id: String,
    pub sender: String,
    pub subject: Option<String>,
    pub status: EmailStatus,
}

#[derive(Debug)]
pub enum EmailStatus {
    Parsed { bill_id: Uuid },
    Skipped(String),
    Failed(String),
}

/// Poll the IMAP mailbox for new bills and process them.
///
/// Connects to the configured IMAP server, searches for unread messages from
/// known utility providers, downloads PDF attachments, extracts text, parses
/// with LLM, validates, and inserts into the database.
///
/// Implementation note: uses `std::process::Command` to shell out to `curl`
/// for IMAP operations, avoiding the need for a Rust IMAP client library
/// (the stable Rust IMAP ecosystem is still maturing for async/tokio).
pub async fn poll_and_ingest(
    config: &ImapConfig,
    pool: &PgPool,
    account_id: Uuid,
    provider: &LlmClient,
) -> Result<Vec<EmailIngestResult>, AiError> {
    // Use curl's IMAP support for fetching emails.
    let config = config.clone();
    let fetched = tokio::task::spawn_blocking(move || fetch_bill_emails(&config))
        .await
        .map_err(|e| AiError::Imap(format!("Task join error: {e}")))?
        ?;

    let mut results = Vec::new();

    for (message_id, sender, subject, pdf_data) in fetched {
        match process_pdf_data(&pdf_data, account_id, provider, pool).await {
            Ok(bill_id) => {
                results.push(EmailIngestResult {
                    message_id,
                    sender,
                    subject,
                    status: EmailStatus::Parsed { bill_id },
                });
            }
            Err(e) => {
                results.push(EmailIngestResult {
                    message_id,
                    sender,
                    subject,
                    status: EmailStatus::Failed(format!("{e}")),
                });
            }
        }
    }

    Ok(results)
}

/// Fetch bill emails using curl's IMAP support.
fn fetch_bill_emails(
    config: &ImapConfig,
) -> Result<Vec<(String, String, Option<String>, Vec<u8>)>, AiError> {
    // List unseen messages using curl.
    let imap_url = format!(
        "imaps://{}:{}/{}",
        config.server, config.port, config.folder
    );

    let output = std::process::Command::new("curl")
        .arg("--silent")
        .arg("--user")
        .arg(format!("{}:{}", config.username, config.password))
        .arg("--url")
        .arg(format!("{imap_url}?UNSEEN"))
        .output()
        .map_err(|e| AiError::Imap(format!("curl not available: {e}")))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(AiError::Imap(format!("IMAP search failed: {stderr}")));
    }

    let listing = String::from_utf8_lossy(&output.stdout);
    let uids: Vec<&str> = listing
        .lines()
        .filter(|l| !l.is_empty())
        .filter_map(|l| l.split_whitespace().last())
        .collect();

    tracing::info!("Found {} unseen messages", uids.len());

    let mut results = Vec::new();

    for uid in uids {
        // Fetch full message.
        let msg_output = std::process::Command::new("curl")
            .arg("--silent")
            .arg("--user")
            .arg(format!("{}:{}", config.username, config.password))
            .arg("--url")
            .arg(format!("{imap_url};UID={uid}"))
            .output()
            .map_err(|e| AiError::Imap(format!("Fetch message failed: {e}")))?;

        if !msg_output.status.success() {
            continue;
        }

        let raw = String::from_utf8_lossy(&msg_output.stdout);

        let sender = extract_header(&raw, "From").unwrap_or_default();
        let subject = extract_header(&raw, "Subject");
        let message_id =
            extract_header(&raw, "Message-ID").unwrap_or_else(|| uid.to_string());

        // Check sender matches utility patterns.
        let sender_lower = sender.to_lowercase();
        let is_utility = config
            .sender_patterns
            .iter()
            .any(|p| sender_lower.contains(p));

        if !is_utility {
            continue;
        }

        // Extract PDF attachment.
        if let Ok(pdf_data) = extract_pdf_attachment(&raw) {
            results.push((message_id, sender, subject, pdf_data));
        }
    }

    Ok(results)
}

async fn process_pdf_data(
    pdf_data: &[u8],
    account_id: Uuid,
    provider: &LlmClient,
    pool: &PgPool,
) -> Result<Uuid, AiError> {
    let tmp_dir = std::env::temp_dir().join("lothal_ingest");
    tokio::fs::create_dir_all(&tmp_dir).await?;

    let tmp_path = tmp_dir.join(format!("{}.pdf", Uuid::new_v4()));
    tokio::fs::write(&tmp_path, pdf_data).await?;

    let text = super::extract_text_from_pdf(&tmp_path)?;
    let bill = super::parse_bill_with_llm(&text, account_id, provider).await?;

    lothal_db::bill::insert_bill(pool, &bill).await?;
    let _ = tokio::fs::remove_file(&tmp_path).await;

    Ok(bill.id)
}

fn extract_pdf_attachment(email: &str) -> Result<Vec<u8>, AiError> {
    let lower = email.to_lowercase();
    let pdf_marker = "content-type: application/pdf";

    let pos = lower
        .find(pdf_marker)
        .ok_or_else(|| AiError::Imap("No PDF attachment found in email".into()))?;

    let after_marker = &email[pos..];

    let header_end = after_marker
        .find("\r\n\r\n")
        .map(|i| i + 4)
        .or_else(|| after_marker.find("\n\n").map(|i| i + 2))
        .ok_or_else(|| AiError::Imap("Malformed MIME: no blank line after PDF headers".into()))?;

    let base64_content = &after_marker[header_end..];
    let end = base64_content
        .find("\r\n--")
        .or_else(|| base64_content.find("\n--"))
        .unwrap_or(base64_content.len());

    let b64_str: String = base64_content[..end]
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect();

    let decoded = base64_decode(&b64_str)?;

    if decoded.len() < 4 || &decoded[..4] != b"%PDF" {
        return Err(AiError::Imap("Decoded content is not a valid PDF".into()));
    }

    Ok(decoded)
}

fn base64_decode(input: &str) -> Result<Vec<u8>, AiError> {
    let bytes = input.as_bytes();
    let mut output = Vec::with_capacity(bytes.len() * 3 / 4);
    let mut buf = 0u32;
    let mut bits = 0u32;

    for &b in bytes {
        let val = match b {
            b'A'..=b'Z' => b - b'A',
            b'a'..=b'z' => b - b'a' + 26,
            b'0'..=b'9' => b - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            b'=' | b'\n' | b'\r' | b' ' => continue,
            _ => return Err(AiError::Imap(format!("Invalid base64 byte: {b}"))),
        };

        buf = (buf << 6) | u32::from(val);
        bits += 6;

        if bits >= 8 {
            bits -= 8;
            output.push((buf >> bits) as u8);
            buf &= (1 << bits) - 1;
        }
    }

    Ok(output)
}

fn extract_header(headers: &str, name: &str) -> Option<String> {
    let prefix_lower = format!("{}: ", name.to_lowercase());
    for line in headers.lines() {
        if line.to_lowercase().starts_with(&prefix_lower) {
            return Some(line[prefix_lower.len()..].trim().to_string());
        }
    }
    None
}

fn required_env(key: &str) -> Result<String, AiError> {
    std::env::var(key).map_err(|_| {
        AiError::ProviderNotConfigured(format!("{key} must be set for email ingest"))
    })
}
