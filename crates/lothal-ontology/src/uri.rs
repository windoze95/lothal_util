use std::fmt;

use anyhow::{anyhow, Context};
use uuid::Uuid;

/// A URI that uniquely identifies an ontology object.
///
/// Format: `lothal://{kind}/{id}`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ObjectUri {
    pub kind: String,
    pub id: Uuid,
}

impl ObjectUri {
    pub fn new(kind: impl Into<String>, id: Uuid) -> Self {
        Self {
            kind: kind.into(),
            id,
        }
    }

    /// Parse a URI of the form `lothal://{kind}/{id}`.
    pub fn parse(s: &str) -> Result<Self, anyhow::Error> {
        let rest = s
            .strip_prefix("lothal://")
            .ok_or_else(|| anyhow!("ObjectUri must start with `lothal://`: {s}"))?;
        let (kind, id_str) = rest
            .split_once('/')
            .ok_or_else(|| anyhow!("ObjectUri missing `/` between kind and id: {s}"))?;
        if kind.is_empty() {
            return Err(anyhow!("ObjectUri has empty kind: {s}"));
        }
        let id = Uuid::parse_str(id_str)
            .with_context(|| format!("ObjectUri has invalid UUID: {id_str}"))?;
        Ok(Self {
            kind: kind.to_string(),
            id,
        })
    }
}

impl fmt::Display for ObjectUri {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "lothal://{}/{}", self.kind, self.id)
    }
}
