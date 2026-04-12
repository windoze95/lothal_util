pub mod bill;
pub mod csv_import;
pub mod ecobee;
pub mod flume;
pub mod green_button;
pub mod mqtt;
pub mod nws;

#[derive(Debug, thiserror::Error)]
pub enum IngestError {
    #[error("PDF extraction failed: {0}")]
    PdfExtraction(String),
    #[error("Parse error: {0}")]
    Parse(String),
    #[error("Provider not recognized")]
    UnknownProvider,
    #[error("Validation error: {0}")]
    Validation(String),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),
    #[error("XML error: {0}")]
    Xml(#[from] quick_xml::DeError),
    #[error("CSV error: {0}")]
    Csv(#[from] csv::Error),
    #[error("MQTT error: {0}")]
    Mqtt(String),
}
