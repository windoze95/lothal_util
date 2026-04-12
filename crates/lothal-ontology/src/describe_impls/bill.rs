use lothal_core::ontology::bill::Bill;
use uuid::Uuid;

use crate::object::Describe;

impl Describe for Bill {
    const KIND: &'static str = "bill";

    fn id(&self) -> Uuid {
        self.id
    }

    fn site_id(&self) -> Option<Uuid> {
        // Bills are anchored to a utility account; site is resolved through
        // the account via the indexer.
        None
    }

    fn display_name(&self) -> String {
        format!(
            "{} to {} - {}",
            self.period.range.start, self.period.range.end, self.total_amount
        )
    }
}
