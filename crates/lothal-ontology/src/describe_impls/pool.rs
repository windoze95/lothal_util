use lothal_core::ontology::water::Pool;
use uuid::Uuid;

use crate::object::Describe;

impl Describe for Pool {
    const KIND: &'static str = "pool";

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
