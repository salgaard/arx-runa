//! Sharing-store implementation backed by SQLCipher metadata storage.

use async_trait::async_trait;
use rusqlite::{OptionalExtension, params};
use uuid::Uuid;

use crate::sharing::{
    Contact, ContactId, DisplayName, SharingError, SharingStore, X25519PublicKey,
};
use crate::storage::{SqlCipherMetadataStore, StorageError};

/// Maps one SQL contact row into the sharing contact domain type.
fn map_contact_row(
    row: (String, String, Option<String>, Vec<u8>, i64),
) -> Result<Contact, SharingError> {
    let (contact_id_text, display_name_text, email, public_key_blob, created_at) = row;
    let parsed_contact_id = Uuid::parse_str(&contact_id_text)
        .map_err(|_| SharingError::InvalidContactId(contact_id_text.clone()))?;
    if !ContactId::from_uuid(parsed_contact_id).is_uuid_v4() {
        return Err(SharingError::InvalidContactId(contact_id_text));
    }
    let public_key_bytes: [u8; 32] = public_key_blob
        .try_into()
        .map_err(|blob: Vec<u8>| SharingError::InvalidPublicKeyLength(blob.len()))?;

    Ok(Contact {
        contact_id: ContactId::from_uuid(parsed_contact_id),
        display_name: DisplayName::new(&display_name_text)?,
        email,
        public_key: X25519PublicKey::new(public_key_bytes),
        created_at,
    })
}

/// Converts storage-domain failures into sharing-domain errors.
fn map_storage_error(error: StorageError) -> SharingError {
    match error {
        StorageError::NotFound => SharingError::ContactNotFound,
        StorageError::ConstraintViolation(message) => SharingError::ConstraintViolation(message),
        _ => SharingError::Backend("storage backend failure".to_owned()),
    }
}

#[async_trait]
impl SharingStore for SqlCipherMetadataStore {
    /// Fetches the owner's X25519 public key from the singleton identity row.
    async fn get_own_public_key(&self) -> Result<X25519PublicKey, SharingError> {
        let public_key_blob = self
            .with_connection_blocking(move |connection| {
                connection
                    .query_row(
                        "SELECT public_key FROM vault_identity WHERE id = 1",
                        [],
                        |row| row.get::<_, Vec<u8>>(0),
                    )
                    .optional()
                    .map_err(StorageError::from_rusqlite)
            })
            .await
            .map_err(map_storage_error)?
            .ok_or(SharingError::IdentityMissing)?;

        let public_key_bytes: [u8; 32] = public_key_blob
            .try_into()
            .map_err(|blob: Vec<u8>| SharingError::InvalidPublicKeyLength(blob.len()))?;
        Ok(X25519PublicKey::new(public_key_bytes))
    }

    /// Inserts one contact row in SQLCipher.
    async fn insert_contact(&self, contact: &Contact) -> Result<(), SharingError> {
        if !contact.contact_id.is_uuid_v4() {
            return Err(SharingError::InvalidContactId(
                contact.contact_id.to_uuid().hyphenated().to_string(),
            ));
        }
        let contact_id_text = contact.contact_id.to_uuid().hyphenated().to_string();
        let display_name_text = contact.display_name.as_str().to_owned();
        let email = contact.email.clone();
        let public_key_blob = contact.public_key.as_bytes().to_vec();
        let created_at = contact.created_at;

        self.with_connection_blocking(move |connection| {
            connection
                .execute(
                    "INSERT INTO contacts (contact_id, display_name, email, public_key, created_at) VALUES (?1, ?2, ?3, ?4, ?5)",
                    params![contact_id_text, display_name_text, email, public_key_blob, created_at],
                )
                .map_err(StorageError::from_rusqlite)?;
            Ok(())
        })
        .await
        .map_err(map_storage_error)
    }

    /// Fetches one contact by identifier.
    async fn get_contact(&self, contact_id: ContactId) -> Result<Contact, SharingError> {
        if !contact_id.is_uuid_v4() {
            return Err(SharingError::InvalidContactId(
                contact_id.to_uuid().hyphenated().to_string(),
            ));
        }
        let contact_id_text = contact_id.to_uuid().hyphenated().to_string();
        let row = self
            .with_connection_blocking(move |connection| {
                connection
                    .query_row(
                        "SELECT contact_id, display_name, email, public_key, created_at FROM contacts WHERE contact_id = ?1",
                        params![contact_id_text],
                        |row| {
                            Ok((
                                row.get::<_, String>(0)?,
                                row.get::<_, String>(1)?,
                                row.get::<_, Option<String>>(2)?,
                                row.get::<_, Vec<u8>>(3)?,
                                row.get::<_, i64>(4)?,
                            ))
                        },
                    )
                    .optional()
                    .map_err(StorageError::from_rusqlite)?
                    .ok_or(StorageError::NotFound)
            })
            .await
            .map_err(map_storage_error)?;
        map_contact_row(row)
    }

    /// Lists contacts in stable deterministic order.
    async fn list_contacts(&self) -> Result<Vec<Contact>, SharingError> {
        let rows = self
            .with_connection_blocking(move |connection| {
                let mut statement = connection
                    .prepare(
                        "SELECT contact_id, display_name, email, public_key, created_at FROM contacts ORDER BY display_name ASC, contact_id ASC",
                    )
                    .map_err(StorageError::from_rusqlite)?;
                let mapped_rows = statement
                    .query_map([], |row| {
                        Ok((
                            row.get::<_, String>(0)?,
                            row.get::<_, String>(1)?,
                            row.get::<_, Option<String>>(2)?,
                            row.get::<_, Vec<u8>>(3)?,
                            row.get::<_, i64>(4)?,
                        ))
                    })
                    .map_err(StorageError::from_rusqlite)?;
                let mut rows = Vec::new();
                for row in mapped_rows {
                    rows.push(row.map_err(StorageError::from_rusqlite)?);
                }
                Ok(rows)
            })
            .await
            .map_err(map_storage_error)?;
        rows.into_iter().map(map_contact_row).collect()
    }

    /// Deletes one contact row by identifier.
    async fn delete_contact(&self, contact_id: ContactId) -> Result<(), SharingError> {
        if !contact_id.is_uuid_v4() {
            return Err(SharingError::InvalidContactId(
                contact_id.to_uuid().hyphenated().to_string(),
            ));
        }
        let contact_id_text = contact_id.to_uuid().hyphenated().to_string();
        self.with_connection_blocking(move |connection| {
            let rows_affected = connection
                .execute(
                    "DELETE FROM contacts WHERE contact_id = ?1",
                    params![contact_id_text],
                )
                .map_err(StorageError::from_rusqlite)?;
            if rows_affected == 0 {
                return Err(StorageError::NotFound);
            }
            Ok(())
        })
        .await
        .map_err(map_storage_error)
    }
}

#[cfg(test)]
mod tests {
    use rusqlite::params;
    use tempfile::tempdir;
    use uuid::Uuid;

    use crate::sharing::{
        Contact, ContactId, DisplayName, SharingError, SharingStore, X25519PublicKey,
    };
    use crate::storage::{SqlCipherMetadataStore, StorageError};

    /// Builds a fresh SQLCipher metadata store for sharing tests.
    async fn create_store() -> (tempfile::TempDir, SqlCipherMetadataStore) {
        let directory = tempdir().expect("tempdir should be created");
        let database_path = directory.path().join("manifest.db");
        let store = SqlCipherMetadataStore::create(
            &database_path,
            &[21u8; 32],
            Uuid::new_v4(),
            4_194_304,
            false,
        )
        .await
        .expect("store should be created");
        (directory, store)
    }

    /// Seeds one `vault_identity` row for identity lookup tests.
    async fn seed_vault_identity(store: &SqlCipherMetadataStore, public_key_blob: Vec<u8>) {
        store
            .with_connection_blocking(move |connection| {
                connection
                    .execute(
                        "INSERT INTO vault_identity (id, public_key, wrapped_private_key) VALUES (1, ?1, ?2)",
                        params![public_key_blob, vec![7u8; 64]],
                    )
                    .map_err(StorageError::from_rusqlite)?;
                Ok(())
            })
            .await
            .expect("identity row insert should succeed");
    }

    /// Creates one contact test fixture.
    fn sample_contact(display_name: &str, email: Option<&str>, marker: u8) -> Contact {
        Contact {
            contact_id: ContactId::from_uuid(Uuid::new_v4()),
            display_name: DisplayName::new(display_name).expect("display name should validate"),
            email: email.map(str::to_owned),
            public_key: X25519PublicKey::new([marker; 32]),
            created_at: 1_700_000_000 + marker as i64,
        }
    }

    /// Verifies the owner's public key round-trips from `vault_identity`.
    #[tokio::test]
    async fn test_sharing_store_get_own_public_key_with_seeded_identity_returns_public_key() {
        let (_directory, store) = create_store().await;
        seed_vault_identity(&store, vec![3u8; 32]).await;

        let public_key = store
            .get_own_public_key()
            .await
            .expect("public key lookup should succeed");
        assert_eq!(public_key.as_bytes(), &[3u8; 32]);
    }

    /// Verifies missing identity row maps to `IdentityMissing`.
    #[tokio::test]
    async fn test_sharing_store_get_own_public_key_without_identity_returns_identity_missing() {
        let (_directory, store) = create_store().await;

        let result = store.get_own_public_key().await;
        assert!(matches!(result, Err(SharingError::IdentityMissing)));
    }

    /// Verifies non-32-byte identity blobs are rejected safely.
    #[tokio::test]
    async fn test_sharing_store_get_own_public_key_with_invalid_blob_length_returns_error() {
        let (_directory, store) = create_store().await;
        seed_vault_identity(&store, vec![9u8; 31]).await;

        let result = store.get_own_public_key().await;
        assert!(matches!(
            result,
            Err(SharingError::InvalidPublicKeyLength(actual_length)) if actual_length == 31
        ));
    }

    /// Verifies contact insert, get, list, and delete round-trip.
    #[tokio::test]
    async fn test_sharing_store_contact_crud_round_trip_preserves_fields() {
        let (_directory, store) = create_store().await;
        let alice_contact = sample_contact("Alice", Some("alice@example.com"), 11);

        store
            .insert_contact(&alice_contact)
            .await
            .expect("insert should succeed");
        let fetched_contact = store
            .get_contact(alice_contact.contact_id)
            .await
            .expect("get should succeed");
        assert_eq!(fetched_contact.contact_id, alice_contact.contact_id);
        assert_eq!(fetched_contact.display_name, alice_contact.display_name);
        assert_eq!(fetched_contact.email, alice_contact.email);
        assert_eq!(fetched_contact.public_key, alice_contact.public_key);
        assert_eq!(fetched_contact.created_at, alice_contact.created_at);

        let listed_contacts = store.list_contacts().await.expect("list should succeed");
        assert_eq!(listed_contacts.len(), 1);
        assert_eq!(listed_contacts[0], alice_contact);

        store
            .delete_contact(alice_contact.contact_id)
            .await
            .expect("delete should succeed");
        assert!(
            store
                .list_contacts()
                .await
                .expect("list after delete should succeed")
                .is_empty()
        );
    }

    /// Verifies list ordering is stable by display name then contact identifier.
    #[tokio::test]
    async fn test_sharing_store_list_contacts_orders_by_display_name_then_contact_id() {
        let (_directory, store) = create_store().await;
        let mut zed_contact = sample_contact("Zed", None, 1);
        zed_contact.contact_id = ContactId::from_uuid(
            Uuid::parse_str("00000000-0000-4000-8000-000000000002").expect("uuid should parse"),
        );
        let mut amy_contact = sample_contact("Amy", None, 2);
        amy_contact.contact_id = ContactId::from_uuid(
            Uuid::parse_str("00000000-0000-4000-8000-000000000001").expect("uuid should parse"),
        );

        store
            .insert_contact(&zed_contact)
            .await
            .expect("insert zed should succeed");
        store
            .insert_contact(&amy_contact)
            .await
            .expect("insert amy should succeed");

        let listed_contacts = store.list_contacts().await.expect("list should succeed");
        assert_eq!(listed_contacts.len(), 2);
        assert_eq!(listed_contacts[0].display_name.as_str(), "Amy");
        assert_eq!(listed_contacts[1].display_name.as_str(), "Zed");
    }

    /// Verifies duplicate contact identifiers map to a constraint violation.
    #[tokio::test]
    async fn test_sharing_store_insert_contact_with_duplicate_contact_id_returns_constraint_violation()
     {
        let (_directory, store) = create_store().await;
        let fixed_contact_id = ContactId::from_uuid(
            Uuid::parse_str("00000000-0000-4000-8000-00000000002a").expect("uuid should parse"),
        );
        let mut first_contact = sample_contact("Alpha", None, 1);
        first_contact.contact_id = fixed_contact_id;
        let mut second_contact = sample_contact("Beta", None, 2);
        second_contact.contact_id = fixed_contact_id;

        store
            .insert_contact(&first_contact)
            .await
            .expect("first insert should succeed");
        let result = store.insert_contact(&second_contact).await;

        assert!(matches!(result, Err(SharingError::ConstraintViolation(_))));
    }

    /// Verifies deleting a missing contact returns `ContactNotFound`.
    #[tokio::test]
    async fn test_sharing_store_delete_contact_for_missing_contact_returns_contact_not_found() {
        let (_directory, store) = create_store().await;
        let result = store
            .delete_contact(ContactId::from_uuid(Uuid::new_v4()))
            .await;

        assert!(matches!(result, Err(SharingError::ContactNotFound)));
    }

    /// Verifies missing contact lookup returns `ContactNotFound`.
    #[tokio::test]
    async fn test_sharing_store_get_contact_for_missing_contact_returns_contact_not_found() {
        let (_directory, store) = create_store().await;
        let result = store
            .get_contact(ContactId::from_uuid(Uuid::new_v4()))
            .await;

        assert!(matches!(result, Err(SharingError::ContactNotFound)));
    }

    /// Verifies non-v4 IDs are rejected before get-contact SQL lookup.
    #[tokio::test]
    async fn test_sharing_store_get_contact_with_non_v4_contact_id_returns_invalid_contact_id() {
        let (_directory, store) = create_store().await;
        let result = store.get_contact(ContactId::new([0u8; 16])).await;

        assert!(matches!(result, Err(SharingError::InvalidContactId(_))));
    }

    /// Verifies empty-string and null email values round-trip distinctly.
    #[tokio::test]
    async fn test_sharing_store_email_some_empty_and_none_round_trip_distinctly() {
        let (_directory, store) = create_store().await;
        let first_contact = sample_contact("EmptyEmail", Some(""), 1);
        let second_contact = sample_contact("NoEmail", None, 2);

        store
            .insert_contact(&first_contact)
            .await
            .expect("first insert should succeed");
        store
            .insert_contact(&second_contact)
            .await
            .expect("second insert should succeed");

        let listed_contacts = store.list_contacts().await.expect("list should succeed");
        let empty_email_contact = listed_contacts
            .iter()
            .find(|contact| contact.contact_id == first_contact.contact_id)
            .expect("empty-email contact should exist");
        let none_email_contact = listed_contacts
            .iter()
            .find(|contact| contact.contact_id == second_contact.contact_id)
            .expect("none-email contact should exist");

        assert_eq!(empty_email_contact.email, Some(String::new()));
        assert_eq!(none_email_contact.email, None);
    }

    /// Verifies non-v4 identifiers are rejected before SQL execution.
    #[tokio::test]
    async fn test_sharing_store_insert_contact_with_non_v4_contact_id_returns_invalid_contact_id() {
        let (_directory, store) = create_store().await;
        let mut invalid_contact = sample_contact("InvalidId", None, 4);
        invalid_contact.contact_id = ContactId::new([0u8; 16]);

        let result = store.insert_contact(&invalid_contact).await;
        assert!(matches!(result, Err(SharingError::InvalidContactId(_))));
    }

    /// Verifies non-v4 IDs are rejected before delete-contact SQL execution.
    #[tokio::test]
    async fn test_sharing_store_delete_contact_with_non_v4_contact_id_returns_invalid_contact_id() {
        let (_directory, store) = create_store().await;
        let result = store.delete_contact(ContactId::new([0u8; 16])).await;

        assert!(matches!(result, Err(SharingError::InvalidContactId(_))));
    }

    /// Verifies malformed persisted UUID strings are rejected during mapping.
    #[tokio::test]
    async fn test_sharing_store_list_contacts_with_malformed_contact_id_returns_invalid_contact_id()
    {
        let (_directory, store) = create_store().await;
        store
            .with_connection_blocking(move |connection| {
                connection
                    .execute(
                        "INSERT INTO contacts (contact_id, display_name, email, public_key, created_at) VALUES (?1, ?2, ?3, ?4, ?5)",
                        params![
                            "not-a-uuid",
                            "Legacy",
                            Option::<String>::None,
                            vec![8u8; 32],
                            123_i64
                        ],
                    )
                    .map_err(StorageError::from_rusqlite)?;
                Ok(())
            })
            .await
            .expect("manual insert should succeed");

        let result = store.list_contacts().await;
        assert!(matches!(
            result,
            Err(SharingError::InvalidContactId(contact_id)) if contact_id == "not-a-uuid"
        ));
    }

    /// Verifies reading persisted non-v4 IDs returns `InvalidContactId`.
    #[tokio::test]
    async fn test_sharing_store_list_contacts_with_persisted_non_v4_id_returns_invalid_contact_id()
    {
        let (_directory, store) = create_store().await;
        store
            .with_connection_blocking(move |connection| {
                connection
                    .execute(
                        "INSERT INTO contacts (contact_id, display_name, email, public_key, created_at) VALUES (?1, ?2, ?3, ?4, ?5)",
                        params![
                            "00000000-0000-1000-8000-000000000001",
                            "Legacy",
                            Option::<String>::None,
                            vec![8u8; 32],
                            123_i64
                        ],
                    )
                    .map_err(StorageError::from_rusqlite)?;
                Ok(())
            })
            .await
            .expect("manual insert should succeed");

        let result = store.list_contacts().await;
        assert!(matches!(result, Err(SharingError::InvalidContactId(_))));
    }

    /// Verifies persisted invalid key blobs are rejected during contact reads.
    #[tokio::test]
    async fn test_sharing_store_get_contact_with_invalid_public_key_blob_returns_invalid_public_key_length()
     {
        let (_directory, store) = create_store().await;
        let contact_id = Uuid::new_v4().hyphenated().to_string();
        let contact_id_for_lookup = ContactId::from_uuid(
            Uuid::parse_str(&contact_id).expect("generated UUID should parse"),
        );
        store
            .with_connection_blocking(move |connection| {
                connection
                    .execute(
                        "INSERT INTO contacts (contact_id, display_name, email, public_key, created_at) VALUES (?1, ?2, ?3, ?4, ?5)",
                        params![
                            contact_id,
                            "Alice",
                            Option::<String>::None,
                            vec![1u8; 31],
                            123_i64
                        ],
                    )
                    .map_err(StorageError::from_rusqlite)?;
                Ok(())
            })
            .await
            .expect("manual insert should succeed");

        let result = store.get_contact(contact_id_for_lookup).await;
        assert!(matches!(
            result,
            Err(SharingError::InvalidPublicKeyLength(actual_length)) if actual_length == 31
        ));
    }

    /// Verifies persisted empty/whitespace display names are rejected during mapping.
    #[tokio::test]
    async fn test_sharing_store_get_contact_with_whitespace_display_name_returns_empty_display_name()
     {
        let (_directory, store) = create_store().await;
        let contact_uuid = Uuid::new_v4();
        let contact_id_text = contact_uuid.hyphenated().to_string();
        let contact_id = ContactId::from_uuid(contact_uuid);
        store
            .with_connection_blocking(move |connection| {
                connection
                    .execute(
                        "INSERT INTO contacts (contact_id, display_name, email, public_key, created_at) VALUES (?1, ?2, ?3, ?4, ?5)",
                        params![
                            contact_id_text,
                            "   ",
                            Option::<String>::None,
                            vec![2u8; 32],
                            123_i64
                        ],
                    )
                    .map_err(StorageError::from_rusqlite)?;
                Ok(())
            })
            .await
            .expect("manual insert should succeed");

        let result = store.get_contact(contact_id).await;
        assert!(matches!(result, Err(SharingError::EmptyDisplayName)));
    }
}
