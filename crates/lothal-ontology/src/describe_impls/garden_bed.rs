use lothal_core::ontology::garden::GardenBed;
use uuid::Uuid;

use crate::object::Describe;

impl Describe for GardenBed {
    const KIND: &'static str = "garden_bed";

    fn id(&self) -> Uuid {
        self.id
    }

    fn site_id(&self) -> Option<Uuid> {
        Some(self.site_id)
    }

    fn display_name(&self) -> String {
        self.name.clone()
    }
}
