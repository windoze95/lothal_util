use serde::{Deserialize, Serialize};
use std::fmt;
use std::ops::{Add, Div, Mul, Sub};

macro_rules! unit_type {
    ($name:ident, $suffix:expr) => {
        #[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Default, Serialize, Deserialize)]
        pub struct $name(pub f64);

        impl $name {
            pub fn new(val: f64) -> Self {
                Self(val)
            }

            pub fn value(self) -> f64 {
                self.0
            }

            pub fn zero() -> Self {
                Self(0.0)
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "{:.2} {}", self.0, $suffix)
            }
        }

        impl Add for $name {
            type Output = Self;
            fn add(self, rhs: Self) -> Self {
                Self(self.0 + rhs.0)
            }
        }

        impl Sub for $name {
            type Output = Self;
            fn sub(self, rhs: Self) -> Self {
                Self(self.0 - rhs.0)
            }
        }

        impl Mul<f64> for $name {
            type Output = Self;
            fn mul(self, rhs: f64) -> Self {
                Self(self.0 * rhs)
            }
        }

        impl Div<f64> for $name {
            type Output = Self;
            fn div(self, rhs: f64) -> Self {
                Self(self.0 / rhs)
            }
        }

        impl Div<$name> for $name {
            type Output = f64;
            fn div(self, rhs: $name) -> f64 {
                self.0 / rhs.0
            }
        }

        impl std::iter::Sum for $name {
            fn sum<I: Iterator<Item = Self>>(iter: I) -> Self {
                Self(iter.map(|u| u.0).sum())
            }
        }
    };
}

unit_type!(Kwh, "kWh");
unit_type!(Watts, "W");
unit_type!(Therms, "therms");
unit_type!(Gallons, "gal");
unit_type!(Usd, "USD");
unit_type!(DegreeDays, "°D");
unit_type!(SquareFeet, "sqft");
unit_type!(Acres, "acres");
unit_type!(Pounds, "lbs");
unit_type!(Inches, "in");
unit_type!(Ppm, "ppm");
unit_type!(CubicFeet, "cuft");

/// Compute cooling degree days for a given day.
/// CDD = max(0, avg_temp_f - base_temp_f)
pub fn cooling_degree_days(avg_temp_f: f64, base_temp_f: f64) -> DegreeDays {
    DegreeDays((avg_temp_f - base_temp_f).max(0.0))
}

/// Compute heating degree days for a given day.
/// HDD = max(0, base_temp_f - avg_temp_f)
pub fn heating_degree_days(avg_temp_f: f64, base_temp_f: f64) -> DegreeDays {
    DegreeDays((base_temp_f - avg_temp_f).max(0.0))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_unit_arithmetic() {
        let a = Kwh::new(100.0);
        let b = Kwh::new(50.0);
        assert_eq!((a + b).value(), 150.0);
        assert_eq!((a - b).value(), 50.0);
        assert_eq!((a * 2.0).value(), 200.0);
        assert_eq!((a / 4.0).value(), 25.0);
        assert_eq!(a / b, 2.0);
    }

    #[test]
    fn test_degree_days() {
        assert_eq!(cooling_degree_days(85.0, 65.0).value(), 20.0);
        assert_eq!(cooling_degree_days(60.0, 65.0).value(), 0.0);
        assert_eq!(heating_degree_days(40.0, 65.0).value(), 25.0);
        assert_eq!(heating_degree_days(70.0, 65.0).value(), 0.0);
    }

    #[test]
    fn test_display() {
        assert_eq!(format!("{}", Kwh::new(123.456)), "123.46 kWh");
        assert_eq!(format!("{}", Usd::new(42.1)), "42.10 USD");
    }

    #[test]
    fn test_sum() {
        let vals = vec![Kwh::new(10.0), Kwh::new(20.0), Kwh::new(30.0)];
        let total: Kwh = vals.into_iter().sum();
        assert_eq!(total.value(), 60.0);
    }
}
