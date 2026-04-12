use lothal_core::ontology::site::Site;
use uuid::Uuid;

use crate::object::Describe;

impl Describe for Site {
    const KIND: &'static str = "site";

    fn id(&self) -> Uuid {
        self.id
    }

    fn site_id(&self) -> Option<Uuid> {
        Some(self.id)
    }

    fn display_name(&self) -> String {
        // `Site` has no `name` field; the street address is the most
        // recognizable human identifier.
        if self.address.trim().is_empty() {
            format!("Site at {}, {}", self.city, self.state)
        } else {
            self.address.clone()
        }
    }
}
