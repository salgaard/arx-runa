//! Contact identifier newtype.

use uuid::{Uuid, Variant, Version};

/// Strongly typed identifier for rows in the `contacts` table.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ContactId([u8; 16]);

impl ContactId {
    /// Constructs a contact identifier from raw UUID bytes.
    pub fn new(bytes: [u8; 16]) -> Self {
        Self(bytes)
    }

    /// Constructs a contact identifier from a UUID.
    pub fn from_uuid(uuid: Uuid) -> Self {
        Self::new(*uuid.as_bytes())
    }

    /// Converts this contact identifier back into a UUID.
    pub fn to_uuid(&self) -> Uuid {
        Uuid::from_bytes(self.0)
    }

    /// Returns the inner UUID bytes.
    pub fn as_bytes(&self) -> &[u8; 16] {
        &self.0
    }

    /// Returns whether this identifier is an RFC4122 UUID version 4.
    pub fn is_uuid_v4(&self) -> bool {
        let uuid = self.to_uuid();
        uuid.get_variant() == Variant::RFC4122 && uuid.get_version() == Some(Version::Random)
    }
}

#[cfg(test)]
mod tests {
    use uuid::Uuid;

    use crate::sharing::types::ContactId;

    /// Verifies UUID conversion round-trips through `ContactId`.
    #[test]
    fn test_contact_id_from_uuid_and_to_uuid_round_trip_preserves_value() {
        let uuid = Uuid::new_v4();
        let contact_id = ContactId::from_uuid(uuid);

        assert_eq!(contact_id.to_uuid(), uuid);
    }

    /// Verifies raw-byte construction round-trips through `as_bytes`.
    #[test]
    fn test_contact_id_new_and_as_bytes_round_trip_preserves_bytes() {
        let bytes = [9u8; 16];
        let contact_id = ContactId::new(bytes);

        assert_eq!(contact_id.as_bytes(), &bytes);
    }

    /// Verifies UUID v4 detection differentiates random and non-random UUID versions.
    #[test]
    fn test_contact_id_is_uuid_v4_for_version_4_returns_true_and_for_other_versions_returns_false()
    {
        let version_four_id =
            ContactId::from_uuid(Uuid::parse_str("00000000-0000-4000-8000-000000000001").unwrap());
        let non_version_four_id = ContactId::new([0u8; 16]);

        assert!(version_four_id.is_uuid_v4());
        assert!(!non_version_four_id.is_uuid_v4());
    }
}
