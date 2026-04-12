pub mod guthrie;
pub mod oge;
pub mod ong;

use std::fmt;
use std::path::Path;
use std::process::Command;

use uuid::Uuid;

use lothal_core::Bill;

use crate::IngestError;

/// Supported bill providers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BillProvider {
    Oge,
    Ong,
    GuthrieWater,
}

impl fmt::Display for BillProvider {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Oge => write!(f, "OG&E"),
            Self::Ong => write!(f, "ONG"),
            Self::GuthrieWater => write!(f, "City of Guthrie Water"),
        }
    }
}

/// Detect the bill provider from extracted PDF text.
pub fn detect_provider(text: &str) -> Result<BillProvider, IngestError> {
    let upper = text.to_uppercase();

    if upper.contains("OKLAHOMA GAS AND ELECTRIC") || upper.contains("OG&E") {
        return Ok(BillProvider::Oge);
    }

    if upper.contains("OKLAHOMA NATURAL GAS") || upper.contains("ONG") {
        return Ok(BillProvider::Ong);
    }

    if upper.contains("CITY OF GUTHRIE") {
        return Ok(BillProvider::GuthrieWater);
    }

    // Regex fallback for "GUTHRIE" followed by "WATER" with anything in between.
    let re = regex::Regex::new(r"(?i)GUTHRIE.*WATER").expect("valid regex");
    if re.is_match(text) {
        return Ok(BillProvider::GuthrieWater);
    }

    Err(IngestError::UnknownProvider)
}

/// Shell out to `pdftotext` (poppler) to extract text from a PDF.
pub fn extract_text_from_pdf(path: &Path) -> Result<String, IngestError> {
    let output = Command::new("pdftotext")
        .arg("-layout")
        .arg(path.as_os_str())
        .arg("-")
        .output()
        .map_err(|e| {
            IngestError::PdfExtraction(format!("failed to run pdftotext: {e}"))
        })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(IngestError::PdfExtraction(format!(
            "pdftotext exited with {}: {stderr}",
            output.status,
        )));
    }

    String::from_utf8(output.stdout).map_err(|e| {
        IngestError::PdfExtraction(format!("pdftotext produced invalid UTF-8: {e}"))
    })
}

/// Extract text from a PDF, detect its provider, and dispatch to the appropriate parser.
pub fn parse_bill(path: &Path, account_id: Uuid) -> Result<Bill, IngestError> {
    let text = extract_text_from_pdf(path)?;
    let provider = detect_provider(&text)?;

    let mut bill = match provider {
        BillProvider::Oge => oge::parse_oge_bill(&text, account_id)?,
        BillProvider::Ong => ong::parse_ong_bill(&text, account_id)?,
        BillProvider::GuthrieWater => guthrie::parse_guthrie_bill(&text, account_id)?,
    };

    bill.source_file = Some(path.display().to_string());
    Ok(bill)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_oge() {
        let text = "Your OKLAHOMA GAS AND ELECTRIC bill for March 2026";
        assert_eq!(detect_provider(text).unwrap(), BillProvider::Oge);
    }

    #[test]
    fn test_detect_oge_abbrev() {
        let text = "OG&E\nStatement Date: 03/15/2026";
        assert_eq!(detect_provider(text).unwrap(), BillProvider::Oge);
    }

    #[test]
    fn test_detect_ong() {
        let text = "OKLAHOMA NATURAL GAS company\nYour monthly statement";
        assert_eq!(detect_provider(text).unwrap(), BillProvider::Ong);
    }

    #[test]
    fn test_detect_guthrie() {
        let text = "CITY OF GUTHRIE\nWater and Sewer Bill";
        assert_eq!(detect_provider(text).unwrap(), BillProvider::GuthrieWater);
    }

    #[test]
    fn test_detect_guthrie_regex() {
        let text = "Guthrie Municipal Water Department";
        assert_eq!(detect_provider(text).unwrap(), BillProvider::GuthrieWater);
    }

    #[test]
    fn test_detect_unknown() {
        let text = "Some random text that matches nothing";
        assert!(detect_provider(text).is_err());
    }
}
