//! Cloud connection descriptor from the cloud-sync design "Connection Descriptor" section.

use serde::{Deserialize, Serialize};

/// Endpoint metadata describing where cloud transport operations are rooted.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CloudEndpoint {
    pub provider: String,
    pub bucket: String,
    pub region: String,
    pub endpoint: String,
    pub path_prefix: String,
}
