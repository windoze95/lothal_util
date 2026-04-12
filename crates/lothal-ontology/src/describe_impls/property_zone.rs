use lothal_core::ontology::property_zone::PropertyZone;
use uuid::Uuid;

use crate::object::Describe;

impl Describe for PropertyZone {
    const KIND: &'static str = "property_zone";

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
