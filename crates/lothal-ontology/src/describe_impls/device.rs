use lothal_core::ontology::device::Device;
use uuid::Uuid;

use crate::object::Describe;

impl Describe for Device {
    const KIND: &'static str = "device";

    fn id(&self) -> Uuid {
        self.id
    }

    fn site_id(&self) -> Option<Uuid> {
        // `Device` is anchored to a structure, not a site directly; the
        // indexer resolves `site_id` via the `contained_in` link.
        None
    }

    fn display_name(&self) -> String {
        self.name.clone()
    }
}
