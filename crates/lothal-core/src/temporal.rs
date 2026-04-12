use chrono::NaiveDate;
use serde::{Deserialize, Serialize};

/// A date range with inclusive start and exclusive end.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DateRange {
    pub start: NaiveDate,
    pub end: NaiveDate,
}

impl DateRange {
    pub fn new(start: NaiveDate, end: NaiveDate) -> Self {
        Self { start, end }
    }

    /// Number of days in this range.
    pub fn days(&self) -> i64 {
        (self.end - self.start).num_days()
    }

    /// Whether this range overlaps with another.
    pub fn overlaps(&self, other: &DateRange) -> bool {
        self.start < other.end && other.start < self.end
    }

    /// Whether this range fully contains another.
    pub fn contains(&self, other: &DateRange) -> bool {
        self.start <= other.start && self.end >= other.end
    }

    /// Whether a specific date falls within this range.
    pub fn contains_date(&self, date: NaiveDate) -> bool {
        date >= self.start && date < self.end
    }

    /// Iterate over each date in the range.
    pub fn iter_days(&self) -> impl Iterator<Item = NaiveDate> {
        let start = self.start;
        let end = self.end;
        (0..(end - start).num_days()).map(move |i| start + chrono::Duration::days(i))
    }
}

impl std::fmt::Display for DateRange {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} to {}", self.start, self.end)
    }
}

/// A billing period with utility-specific metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BillingPeriod {
    pub range: DateRange,
    pub meter_read_start: Option<NaiveDate>,
    pub meter_read_end: Option<NaiveDate>,
}

impl BillingPeriod {
    pub fn new(start: NaiveDate, end: NaiveDate) -> Self {
        Self {
            range: DateRange::new(start, end),
            meter_read_start: None,
            meter_read_end: None,
        }
    }

    pub fn days(&self) -> i64 {
        self.range.days()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(y: i32, m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, day).unwrap()
    }

    #[test]
    fn test_date_range_days() {
        let r = DateRange::new(d(2026, 1, 1), d(2026, 1, 31));
        assert_eq!(r.days(), 30);
    }

    #[test]
    fn test_overlap() {
        let a = DateRange::new(d(2026, 1, 1), d(2026, 1, 15));
        let b = DateRange::new(d(2026, 1, 10), d(2026, 1, 25));
        let c = DateRange::new(d(2026, 1, 15), d(2026, 1, 30));
        assert!(a.overlaps(&b));
        assert!(!a.overlaps(&c)); // a ends where c starts (exclusive)
    }

    #[test]
    fn test_contains_date() {
        let r = DateRange::new(d(2026, 3, 1), d(2026, 4, 1));
        assert!(r.contains_date(d(2026, 3, 1)));
        assert!(r.contains_date(d(2026, 3, 15)));
        assert!(!r.contains_date(d(2026, 4, 1))); // exclusive end
    }

    #[test]
    fn test_iter_days() {
        let r = DateRange::new(d(2026, 1, 1), d(2026, 1, 4));
        let days: Vec<_> = r.iter_days().collect();
        assert_eq!(
            days,
            vec![d(2026, 1, 1), d(2026, 1, 2), d(2026, 1, 3)]
        );
    }
}
