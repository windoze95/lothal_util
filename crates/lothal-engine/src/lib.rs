pub mod baseline;
pub mod experiment;
pub mod recommend;
pub mod simulate;
pub mod water_baseline;

#[derive(Debug, thiserror::Error)]
pub enum EngineError {
    #[error("Insufficient data: {0}")]
    InsufficientData(String),
    #[error("Computation error: {0}")]
    Computation(String),
    #[error("Invalid input: {0}")]
    InvalidInput(String),
}
