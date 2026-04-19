//! Cloud sync configuration values.

use serde::{Deserialize, Serialize};

use super::CloudTransportError;

/// Sync configuration used for cloud transport operations.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncConfig {
    /// Maximum parallel operations.
    pub max_concurrent: u32,
    /// Operation timeout in seconds.
    pub operation_timeout_seconds: u64,
}

impl Default for SyncConfig {
    fn default() -> Self {
        Self {
            max_concurrent: 4,
            operation_timeout_seconds: 300,
        }
    }
}

impl SyncConfig {
    /// Constructs and validates a `SyncConfig`.
    pub fn new(
        max_concurrent: u32,
        operation_timeout_seconds: u64,
    ) -> Result<Self, CloudTransportError> {
        let config = Self {
            max_concurrent,
            operation_timeout_seconds,
        };
        config.validate()?;
        Ok(config)
    }

    /// Validates the canonical range constraints.
    pub fn validate(&self) -> Result<(), CloudTransportError> {
        if !(1..=16).contains(&self.max_concurrent) {
            return Err(CloudTransportError::Other(
                "sync config invalid: max_concurrent must be in 1..=16".to_owned(),
            ));
        }
        if !(60..=3600).contains(&self.operation_timeout_seconds) {
            return Err(CloudTransportError::Other(
                "sync config invalid: operation_timeout_seconds must be in 60..=3600".to_owned(),
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::SyncConfig;

    #[test]
    fn test_sync_config_default_values_match_design() {
        let config = SyncConfig::default();
        assert_eq!(config.max_concurrent, 4);
        assert_eq!(config.operation_timeout_seconds, 300);
    }

    #[test]
    fn test_sync_config_validate_accepts_boundary_values() {
        assert!(SyncConfig::new(1, 60).is_ok());
        assert!(SyncConfig::new(16, 3600).is_ok());
    }

    #[test]
    fn test_sync_config_validate_rejects_out_of_range_values() {
        assert!(SyncConfig::new(0, 300).is_err());
        assert!(SyncConfig::new(17, 300).is_err());
        assert!(SyncConfig::new(4, 59).is_err());
        assert!(SyncConfig::new(4, 3601).is_err());
    }
}
