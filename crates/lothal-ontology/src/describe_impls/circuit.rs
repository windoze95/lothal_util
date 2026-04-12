use lothal_core::ontology::circuit::Circuit;
use uuid::Uuid;

use crate::object::Describe;

impl Describe for Circuit {
    const KIND: &'static str = "circuit";

    fn id(&self) -> Uuid {
        self.id
    }

    fn site_id(&self) -> Option<Uuid> {
        // Circuits are panel → structure → site; site is resolved by the
        // indexer through link traversal.
        None
    }

    fn display_name(&self) -> String {
        // Include breaker number for disambiguation; labels often repeat.
        format!("Breaker {} - {}", self.breaker_number, self.label)
    }
}
