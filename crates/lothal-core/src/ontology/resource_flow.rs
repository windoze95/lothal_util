use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::units::Usd;

/// A directed flow of a resource between two entities on the property.
///
/// This is the key architectural entity for cross-system loops:
/// - Water: rainfall → roof → cistern → irrigation → garden
/// - Biomass: kitchen scraps → compost → garden; chicken feed → eggs + manure → compost
/// - Energy: grid → panel → device
/// - Nutrients: compost → soil → crops → kitchen → scraps → compost
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceFlow {
    pub id: Uuid,
    pub site_id: Uuid,
    pub resource_type: ResourceType,
    pub source: FlowEndpoint,
    pub sink: FlowEndpoint,
    pub quantity: f64,
    pub unit: String,
    pub cost: Option<Usd>,
    pub timestamp: DateTime<Utc>,
    pub notes: Option<String>,
    pub created_at: DateTime<Utc>,
}

impl ResourceFlow {
    pub fn new(
        site_id: Uuid,
        resource_type: ResourceType,
        source: FlowEndpoint,
        sink: FlowEndpoint,
        quantity: f64,
        unit: String,
    ) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            site_id,
            resource_type,
            source,
            sink,
            quantity,
            unit,
            cost: None,
            timestamp: now,
            notes: None,
            created_at: now,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResourceType {
    Water,
    Electricity,
    Gas,
    Biomass,
    Nutrients,
    Heat,
    Sunlight,
}

impl std::fmt::Display for ResourceType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = serde_json::to_string(self).unwrap_or_else(|_| "unknown".into());
        write!(f, "{}", s.trim_matches('"'))
    }
}

impl std::str::FromStr for ResourceType {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let quoted = format!("\"{s}\"");
        serde_json::from_str(&quoted).map_err(|_| format!("unknown resource type: {s}"))
    }
}

/// A polymorphic endpoint in a resource flow graph.
///
/// Follows the same tagged-enum pattern as `ReadingSource` and `MaintenanceTarget`.
/// In SQL, stored as `(endpoint_type TEXT, endpoint_id UUID)`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", content = "id")]
pub enum FlowEndpoint {
    WaterSource(Uuid),
    Pool(Uuid),
    SepticSystem(Uuid),
    Structure(Uuid),
    PropertyZone(Uuid),
    Device(Uuid),
    GardenBed(Uuid),
    CompostPile(Uuid),
    Flock(Uuid),
    /// Outside the system boundary (e.g. rainfall in, waste out).
    External,
}

impl FlowEndpoint {
    pub fn endpoint_type(&self) -> &'static str {
        match self {
            Self::WaterSource(_) => "water_source",
            Self::Pool(_) => "pool",
            Self::SepticSystem(_) => "septic_system",
            Self::Structure(_) => "structure",
            Self::PropertyZone(_) => "property_zone",
            Self::Device(_) => "device",
            Self::GardenBed(_) => "garden_bed",
            Self::CompostPile(_) => "compost_pile",
            Self::Flock(_) => "flock",
            Self::External => "external",
        }
    }

    pub fn endpoint_id(&self) -> Option<Uuid> {
        match self {
            Self::WaterSource(id)
            | Self::Pool(id)
            | Self::SepticSystem(id)
            | Self::Structure(id)
            | Self::PropertyZone(id)
            | Self::Device(id)
            | Self::GardenBed(id)
            | Self::CompostPile(id)
            | Self::Flock(id) => Some(*id),
            Self::External => None,
        }
    }

    /// Build from SQL columns (type + id). Uses nil UUID for External.
    pub fn from_sql(endpoint_type: &str, endpoint_id: Uuid) -> Self {
        match endpoint_type {
            "water_source" => Self::WaterSource(endpoint_id),
            "pool" => Self::Pool(endpoint_id),
            "septic_system" => Self::SepticSystem(endpoint_id),
            "structure" => Self::Structure(endpoint_id),
            "property_zone" => Self::PropertyZone(endpoint_id),
            "device" => Self::Device(endpoint_id),
            "garden_bed" => Self::GardenBed(endpoint_id),
            "compost_pile" => Self::CompostPile(endpoint_id),
            "flock" => Self::Flock(endpoint_id),
            "external" => Self::External,
            _ => Self::External, // fallback
        }
    }

    /// UUID for SQL storage. External uses nil UUID.
    pub fn sql_id(&self) -> Uuid {
        self.endpoint_id().unwrap_or(Uuid::nil())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resource_type_round_trip() {
        let types = [
            ResourceType::Water,
            ResourceType::Electricity,
            ResourceType::Biomass,
            ResourceType::Nutrients,
        ];
        for t in types {
            let s = t.to_string();
            let parsed: ResourceType = s.parse().unwrap();
            assert_eq!(parsed, t);
        }
    }

    #[test]
    fn test_flow_endpoint_type_and_id() {
        let id = Uuid::new_v4();
        let ep = FlowEndpoint::WaterSource(id);
        assert_eq!(ep.endpoint_type(), "water_source");
        assert_eq!(ep.endpoint_id(), Some(id));
        assert_eq!(ep.sql_id(), id);
    }

    #[test]
    fn test_flow_endpoint_external() {
        let ep = FlowEndpoint::External;
        assert_eq!(ep.endpoint_type(), "external");
        assert_eq!(ep.endpoint_id(), None);
        assert_eq!(ep.sql_id(), Uuid::nil());
    }

    #[test]
    fn test_flow_endpoint_from_sql_round_trip() {
        let id = Uuid::new_v4();
        let ep = FlowEndpoint::CompostPile(id);
        let rebuilt = FlowEndpoint::from_sql(ep.endpoint_type(), ep.sql_id());
        assert_eq!(rebuilt, ep);
    }

    #[test]
    fn test_flow_endpoint_external_from_sql() {
        let ep = FlowEndpoint::from_sql("external", Uuid::nil());
        assert_eq!(ep, FlowEndpoint::External);
    }

    #[test]
    fn test_resource_flow_constructor() {
        let site_id = Uuid::new_v4();
        let src = FlowEndpoint::External;
        let sink = FlowEndpoint::PropertyZone(Uuid::new_v4());
        let flow = ResourceFlow::new(
            site_id,
            ResourceType::Water,
            src,
            sink,
            1400.0,
            "gallons".to_string(),
        );
        assert_eq!(flow.resource_type, ResourceType::Water);
        assert_eq!(flow.quantity, 1400.0);
    }
}
