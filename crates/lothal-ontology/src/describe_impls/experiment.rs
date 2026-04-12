use lothal_core::ontology::experiment::Experiment;
use uuid::Uuid;

use crate::object::Describe;

impl Describe for Experiment {
    const KIND: &'static str = "experiment";

    fn id(&self) -> Uuid {
        self.id
    }

    fn site_id(&self) -> Option<Uuid> {
        Some(self.site_id)
    }

    fn display_name(&self) -> String {
        // `Experiment` has no title of its own; the human-readable label
        // lives on its `Hypothesis`. Summarize with status + result period
        // so the object is still recognizable in raw listings. The indexer
        // can enrich this with the hypothesis title via link traversal.
        format!(
            "Experiment [{}] {} to {}",
            self.status, self.result_period.start, self.result_period.end
        )
    }
}
