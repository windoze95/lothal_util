use lothal_core::ontology::utility::UtilityAccount;
use uuid::Uuid;

use crate::object::Describe;

impl Describe for UtilityAccount {
    const KIND: &'static str = "utility_account";

    fn id(&self) -> Uuid {
        self.id
    }

    fn site_id(&self) -> Option<Uuid> {
        Some(self.site_id)
    }

    fn display_name(&self) -> String {
        // Prefer utility type + provider + last-4 of the account number.
        // Fall back to provider name alone if no account number is set.
        let last4 = self
            .account_number
            .as_deref()
            .map(|n| {
                let trimmed: String = n.chars().filter(|c| !c.is_whitespace()).collect();
                let len = trimmed.chars().count();
                if len >= 4 {
                    let start = trimmed.chars().count() - 4;
                    trimmed.chars().skip(start).collect::<String>()
                } else {
                    trimmed
                }
            })
            .filter(|s| !s.is_empty());

        match last4 {
            Some(tail) => format!("{} ({}) ...{}", self.utility_type, self.provider_name, tail),
            None => format!("{} ({})", self.utility_type, self.provider_name),
        }
    }
}
