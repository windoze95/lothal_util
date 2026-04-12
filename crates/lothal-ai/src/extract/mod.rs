pub mod email;
pub mod schema;
pub mod validate;

use std::path::Path;
use std::process::Command;

use uuid::Uuid;

use crate::provider::LlmClient;
use crate::AiError;

/// Parse a bill from PDF text using LLM structured output extraction.
///
/// The caller is responsible for extracting text from the PDF first (via
/// `extract_text_from_pdf`). This function sends the text to the LLM with a
/// structured schema and validates the result.
pub async fn parse_bill_with_llm(
    text: &str,
    account_id: Uuid,
    provider: &LlmClient,
) -> Result<lothal_core::Bill, AiError> {
    let request = schema::build_extraction_request(text);
    let json_schema = schema::bill_json_schema();

    let raw = provider.complete_json(&request, &json_schema).await?;
    let extracted: schema::ExtractedBill = serde_json::from_value(raw)?;

    validate::validate_and_convert(extracted, account_id, provider, text).await
}

/// Shell out to `pdftotext` (poppler) to extract text from a PDF.
pub fn extract_text_from_pdf(path: &Path) -> Result<String, AiError> {
    let output = Command::new("pdftotext")
        .arg("-layout")
        .arg(path.as_os_str())
        .arg("-")
        .output()
        .map_err(|e| AiError::PdfExtraction(format!("failed to run pdftotext: {e}")))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(AiError::PdfExtraction(format!(
            "pdftotext exited with {}: {stderr}",
            output.status,
        )));
    }

    String::from_utf8(output.stdout)
        .map_err(|e| AiError::PdfExtraction(format!("invalid UTF-8 from pdftotext: {e}")))
}
