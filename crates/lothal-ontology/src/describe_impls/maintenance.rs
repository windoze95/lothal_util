use lothal_core::ontology::maintenance::MaintenanceEvent;
use uuid::Uuid;

use crate::object::Describe;

impl Describe for MaintenanceEvent {
    const KIND: &'static str = "maintenance";

    fn id(&self) -> Uuid {
        self.id
    }

    fn site_id(&self) -> Option<Uuid> {
        // `MaintenanceEvent` hangs off a `MaintenanceTarget` (device,
        // structure, property_zone, pool, tree, septic). Site is resolved
        // through the target via link traversal.
        None
    }

    fn display_name(&self) -> String {
        if self.description.trim().is_empty() {
            format!(
                "{} on {} ({})",
                self.event_type,
                self.target.target_type(),
                self.date
            )
        } else {
            format!("{}: {}", self.event_type, self.description)
        }
    }
}
