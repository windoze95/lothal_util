use lothal_core::ontology::livestock::Flock;
use uuid::Uuid;

use crate::object::Describe;

impl Describe for Flock {
    const KIND: &'static str = "flock";

    fn id(&self) -> Uuid {
        self.id
    }

    fn site_id(&self) -> Option<Uuid> {
        Some(self.site_id)
    }

    fn display_name(&self) -> String {
        if self.name.trim().is_empty() {
            format!("{} flock ({} birds)", self.breed, self.bird_count)
        } else {
            self.name.clone()
        }
    }
}
