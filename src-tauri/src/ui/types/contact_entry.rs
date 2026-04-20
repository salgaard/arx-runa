//! Contact entry response type.

use serde::Serialize;

/// A contact entry returned by `list_contacts` or `add_contact`.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ContactEntry {
    /// Unique contact identifier.
    pub contact_id: String,
    /// Contact's display name.
    pub display_name: String,
    /// Contact's email address, if provided at creation.
    pub email: Option<String>,
    /// ISO 8601 timestamp when the contact was added.
    pub created_at: String,
}
