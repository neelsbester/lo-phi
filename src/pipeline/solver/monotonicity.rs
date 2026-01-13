//! Monotonicity constraints for optimal binning
//!
//! Defines the types of WoE monotonicity patterns that can be enforced
//! during solver-based binning.

use serde::Serialize;

/// Monotonicity constraint for WoE pattern in binning
///
/// These constraints ensure the Weight of Evidence follows a specific
/// pattern across bins, which is important for credit scoring and
/// regulatory compliance.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub enum MonotonicityConstraint {
    /// No monotonicity constraint - WoE can vary freely
    #[default]
    None,
    /// WoE must increase with feature value (higher values = higher risk)
    Ascending,
    /// WoE must decrease with feature value (higher values = lower risk)
    Descending,
    /// WoE increases then decreases (single peak pattern)
    Peak,
    /// WoE decreases then increases (single valley pattern)
    Valley,
    /// Automatically detect the best monotonicity pattern
    Auto,
}

impl std::fmt::Display for MonotonicityConstraint {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MonotonicityConstraint::None => write!(f, "none"),
            MonotonicityConstraint::Ascending => write!(f, "ascending"),
            MonotonicityConstraint::Descending => write!(f, "descending"),
            MonotonicityConstraint::Peak => write!(f, "peak"),
            MonotonicityConstraint::Valley => write!(f, "valley"),
            MonotonicityConstraint::Auto => write!(f, "auto"),
        }
    }
}

impl std::str::FromStr for MonotonicityConstraint {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "none" => Ok(MonotonicityConstraint::None),
            "ascending" | "asc" => Ok(MonotonicityConstraint::Ascending),
            "descending" | "desc" => Ok(MonotonicityConstraint::Descending),
            "peak" => Ok(MonotonicityConstraint::Peak),
            "valley" => Ok(MonotonicityConstraint::Valley),
            "auto" => Ok(MonotonicityConstraint::Auto),
            _ => Err(format!(
                "Unknown monotonicity constraint: '{}'. Use 'none', 'ascending', 'descending', 'peak', 'valley', or 'auto'.",
                s
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_monotonicity_from_str() {
        assert_eq!(
            "none".parse::<MonotonicityConstraint>().unwrap(),
            MonotonicityConstraint::None
        );
        assert_eq!(
            "ascending".parse::<MonotonicityConstraint>().unwrap(),
            MonotonicityConstraint::Ascending
        );
        assert_eq!(
            "asc".parse::<MonotonicityConstraint>().unwrap(),
            MonotonicityConstraint::Ascending
        );
        assert_eq!(
            "descending".parse::<MonotonicityConstraint>().unwrap(),
            MonotonicityConstraint::Descending
        );
        assert_eq!(
            "peak".parse::<MonotonicityConstraint>().unwrap(),
            MonotonicityConstraint::Peak
        );
        assert_eq!(
            "valley".parse::<MonotonicityConstraint>().unwrap(),
            MonotonicityConstraint::Valley
        );
        assert_eq!(
            "auto".parse::<MonotonicityConstraint>().unwrap(),
            MonotonicityConstraint::Auto
        );
    }

    #[test]
    fn test_monotonicity_display() {
        assert_eq!(MonotonicityConstraint::None.to_string(), "none");
        assert_eq!(MonotonicityConstraint::Ascending.to_string(), "ascending");
        assert_eq!(MonotonicityConstraint::Descending.to_string(), "descending");
        assert_eq!(MonotonicityConstraint::Peak.to_string(), "peak");
        assert_eq!(MonotonicityConstraint::Valley.to_string(), "valley");
        assert_eq!(MonotonicityConstraint::Auto.to_string(), "auto");
    }
}
