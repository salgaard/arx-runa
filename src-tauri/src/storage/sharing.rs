//! Sharing-store implementation backed by SQLCipher metadata storage.

use async_trait::async_trait;
use rusqlite::{OptionalExtension, Row, params};
use uuid::Uuid;

use crate::sharing::{
    Contact, ContactId, DisplayName, ReceivedShare, ShareRecord, SharingError, SharingStore,
    X25519PublicKey,
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

    /// Inserts one received-share row in SQLCipher.
    async fn insert_received_share(&self, row: &ReceivedShare) -> Result<(), SharingError> {
        if !is_uuid_v4_string(&row.share_id) {
            return Err(SharingError::InvalidJsonPayload(format!(
                "share_id is not UUID v4: {}",
                row.share_id
            )));
        }
        let share_id = row.share_id.clone();
        let sender_contact_id = row
            .sender_contact_id
            .map(|contact_id| contact_id.to_uuid().hyphenated().to_string());
        let sender_public_key_blob = row.sender_public_key.as_bytes().to_vec();
        let file_id = row.file_id.clone();
        let file_name = row.file_name.clone();
        let file_key_wrapped_blob = row.file_key_wrapped.to_vec();
        let chunk_count = row.chunk_count as i64;
        let chunk_size = row.chunk_size as i64;
        let chunk_uuids_json = serde_json::to_string(&row.chunk_uuids)
            .map_err(|error| SharingError::InvalidJsonPayload(error.to_string()))?;
        let cloud_endpoint_json = serde_json::to_string(&row.cloud_endpoint)
            .map_err(|error| SharingError::InvalidJsonPayload(error.to_string()))?;
        let expires_at = row.expires_at;
        let imported_at = row.imported_at;

        self.with_connection_blocking(move |connection| {
            connection
                .execute(
                    "INSERT INTO received_shares (share_id, sender_contact_id, sender_public_key, file_id, file_name, file_key_wrapped, chunk_count, chunk_size, chunk_uuids, cloud_endpoint, expires_at, imported_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
                    params![
                        share_id,
                        sender_contact_id,
                        sender_public_key_blob,
                        file_id,
                        file_name,
                        file_key_wrapped_blob,
                        chunk_count,
                        chunk_size,
                        chunk_uuids_json,
                        cloud_endpoint_json,
                        expires_at,
                        imported_at,
                    ],
                )
                .map_err(StorageError::from_rusqlite)?;
            Ok(())
        })
        .await
        .map_err(map_storage_error)
    }

    /// Fetches one received-share row by share identifier.
    async fn get_received_share(&self, share_id: &str) -> Result<ReceivedShare, SharingError> {
        let share_id_owned = share_id.to_owned();
        let row = self
            .with_connection_blocking(move |connection| {
                connection
                    .query_row(
                        "SELECT share_id, sender_contact_id, sender_public_key, file_id, file_name, file_key_wrapped, chunk_count, chunk_size, chunk_uuids, cloud_endpoint, expires_at, imported_at FROM received_shares WHERE share_id = ?1",
                        params![share_id_owned],
                        map_received_share_row,
                    )
                    .optional()
                    .map_err(StorageError::from_rusqlite)?
                    .ok_or(StorageError::NotFound)
            })
            .await
            .map_err(map_received_share_error)?;
        parse_received_share_row(row)
    }

    /// Lists all received shares ordered by import time descending.
    async fn list_received_shares(&self) -> Result<Vec<ReceivedShare>, SharingError> {
        let rows = self
            .with_connection_blocking(move |connection| {
                let mut statement = connection
                    .prepare(
                        "SELECT share_id, sender_contact_id, sender_public_key, file_id, file_name, file_key_wrapped, chunk_count, chunk_size, chunk_uuids, cloud_endpoint, expires_at, imported_at FROM received_shares ORDER BY imported_at DESC, share_id ASC",
                    )
                    .map_err(StorageError::from_rusqlite)?;
                let mapped_rows = statement
                    .query_map([], map_received_share_row)
                    .map_err(StorageError::from_rusqlite)?;
                let mut rows = Vec::new();
                for row in mapped_rows {
                    rows.push(row.map_err(StorageError::from_rusqlite)?);
                }
                Ok(rows)
            })
            .await
            .map_err(map_received_share_error)?;
        rows.into_iter().map(parse_received_share_row).collect()
    }

    /// Inserts one outgoing share row.
    async fn insert_share(&self, share: &ShareRecord) -> Result<(), SharingError> {
        let share_id = share.share_id.clone();
        let file_id = share.file_id.clone();
        let contact_id_text = share.contact_id.to_uuid().hyphenated().to_string();
        let file_share_id = share.file_share_id.clone();
        let cloud_path = share.cloud_path.clone();
        let created_at = share.created_at;
        let expires_at = share.expires_at;
        let revoked_at = share.revoked_at;

        self.with_connection_blocking(move |connection| {
            connection
                .execute(
                    "INSERT INTO shares (share_id, file_id, contact_id, file_share_id, cloud_path, created_at, expires_at, revoked_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                    params![share_id, file_id, contact_id_text, file_share_id, cloud_path, created_at, expires_at, revoked_at],
                )
                .map_err(StorageError::from_rusqlite)?;
            Ok(())
        })
        .await
        .map_err(map_share_error)
    }

    /// Fetches one outgoing share row by share identifier.
    async fn get_share(&self, share_id: &str) -> Result<ShareRecord, SharingError> {
        let share_id_owned = share_id.to_owned();
        let row = self
            .with_connection_blocking(move |connection| {
                connection
                    .query_row(
                        "SELECT share_id, file_id, contact_id, file_share_id, cloud_path, created_at, expires_at, revoked_at FROM shares WHERE share_id = ?1",
                        params![share_id_owned],
                        map_share_row,
                    )
                    .optional()
                    .map_err(StorageError::from_rusqlite)?
                    .ok_or(StorageError::NotFound)
            })
            .await
            .map_err(map_share_error)?;
        parse_share_row(row)
    }

    /// Lists all share rows for a given file in deterministic order.
    async fn list_shares_by_file(&self, file_id: &str) -> Result<Vec<ShareRecord>, SharingError> {
        let file_id_owned = file_id.to_owned();
        let rows = self
            .with_connection_blocking(move |connection| {
                let mut statement = connection
                    .prepare(
                        "SELECT share_id, file_id, contact_id, file_share_id, cloud_path, created_at, expires_at, revoked_at FROM shares WHERE file_id = ?1 ORDER BY created_at ASC, share_id ASC",
                    )
                    .map_err(StorageError::from_rusqlite)?;
                let mapped_rows = statement
                    .query_map(params![file_id_owned], map_share_row)
                    .map_err(StorageError::from_rusqlite)?;
                let mut rows = Vec::new();
                for row in mapped_rows {
                    rows.push(row.map_err(StorageError::from_rusqlite)?);
                }
                Ok(rows)
            })
            .await
            .map_err(map_share_error)?;
        rows.into_iter().map(parse_share_row).collect()
    }

    /// Lists only active (non-revoked) share rows for a given file.
    async fn list_active_shares_by_file(
        &self,
        file_id: &str,
    ) -> Result<Vec<ShareRecord>, SharingError> {
        let file_id_owned = file_id.to_owned();
        let rows = self
            .with_connection_blocking(move |connection| {
                let mut statement = connection
                    .prepare(
                        "SELECT share_id, file_id, contact_id, file_share_id, cloud_path, created_at, expires_at, revoked_at FROM shares WHERE file_id = ?1 AND revoked_at IS NULL ORDER BY created_at ASC, share_id ASC",
                    )
                    .map_err(StorageError::from_rusqlite)?;
                let mapped_rows = statement
                    .query_map(params![file_id_owned], map_share_row)
                    .map_err(StorageError::from_rusqlite)?;
                let mut rows = Vec::new();
                for row in mapped_rows {
                    rows.push(row.map_err(StorageError::from_rusqlite)?);
                }
                Ok(rows)
            })
            .await
            .map_err(map_share_error)?;
        rows.into_iter().map(parse_share_row).collect()
    }

    /// Lists only active share rows for a given `file_share_id`.
    async fn list_active_shares_by_file_share_id(
        &self,
        file_share_id: &str,
    ) -> Result<Vec<ShareRecord>, SharingError> {
        let file_share_id_owned = file_share_id.to_owned();
        let rows = self
            .with_connection_blocking(move |connection| {
                let mut statement = connection
                    .prepare(
                        "SELECT share_id, file_id, contact_id, file_share_id, cloud_path, created_at, expires_at, revoked_at FROM shares WHERE file_share_id = ?1 AND revoked_at IS NULL ORDER BY created_at ASC, share_id ASC",
                    )
                    .map_err(StorageError::from_rusqlite)?;
                let mapped_rows = statement
                    .query_map(params![file_share_id_owned], map_share_row)
                    .map_err(StorageError::from_rusqlite)?;
                let mut rows = Vec::new();
                for row in mapped_rows {
                    rows.push(row.map_err(StorageError::from_rusqlite)?);
                }
                Ok(rows)
            })
            .await
            .map_err(map_share_error)?;
        rows.into_iter().map(parse_share_row).collect()
    }

    /// Sets `revoked_at` timestamp on a share row (only if currently active).
    ///
    /// Returns `ShareNotFound` if the `share_id` does not exist, or
    /// `ShareAlreadyRevoked` if `revoked_at IS NOT NULL`.
    async fn set_share_revoked_at(
        &self,
        share_id: &str,
        revoked_at: i64,
    ) -> Result<(), SharingError> {
        let share_id_owned = share_id.to_owned();
        self.with_connection_blocking(move |connection| {
            let tx = connection
                .transaction()
                .map_err(StorageError::from_rusqlite)?;
            let existing_revoked_at = tx
                .query_row(
                    "SELECT revoked_at FROM shares WHERE share_id = ?1",
                    params![share_id_owned.clone()],
                    |row| row.get::<_, Option<i64>>(0),
                )
                .optional()
                .map_err(StorageError::from_rusqlite)?
                .ok_or(StorageError::NotFound)?;
            if existing_revoked_at.is_some() {
                return Err(StorageError::ConstraintViolation(
                    "share already revoked".to_owned(),
                ));
            }
            tx.execute(
                "UPDATE shares SET revoked_at = ?1 WHERE share_id = ?2",
                params![revoked_at, share_id_owned],
            )
            .map_err(StorageError::from_rusqlite)?;
            tx.commit().map_err(StorageError::from_rusqlite)?;
            Ok(())
        })
        .await
        .map_err(|error| match error {
            StorageError::NotFound => SharingError::ShareNotFound,
            StorageError::ConstraintViolation(_) => SharingError::ShareAlreadyRevoked,
            _ => SharingError::Backend("storage backend failure".to_owned()),
        })
    }
}

/// Checks whether a string is a valid hyphenated UUID v4.
fn is_uuid_v4_string(value: &str) -> bool {
    Uuid::parse_str(value)
        .map(|uuid| uuid.get_version_num() == 4)
        .unwrap_or(false)
}

/// Raw row tuple from a `received_shares` SELECT.
type ReceivedShareRow = (
    String,
    Option<String>,
    Vec<u8>,
    String,
    String,
    Vec<u8>,
    i64,
    i64,
    String,
    String,
    Option<i64>,
    i64,
);

/// Maps a single rusqlite row into the raw row tuple.
fn map_received_share_row(row: &Row<'_>) -> rusqlite::Result<ReceivedShareRow> {
    Ok((
        row.get::<_, String>(0)?,
        row.get::<_, Option<String>>(1)?,
        row.get::<_, Vec<u8>>(2)?,
        row.get::<_, String>(3)?,
        row.get::<_, String>(4)?,
        row.get::<_, Vec<u8>>(5)?,
        row.get::<_, i64>(6)?,
        row.get::<_, i64>(7)?,
        row.get::<_, String>(8)?,
        row.get::<_, String>(9)?,
        row.get::<_, Option<i64>>(10)?,
        row.get::<_, i64>(11)?,
    ))
}

/// Converts a raw row tuple into the domain `ReceivedShare`.
fn parse_received_share_row(row: ReceivedShareRow) -> Result<ReceivedShare, SharingError> {
    let (
        share_id,
        sender_contact_id_text,
        sender_public_key_blob,
        file_id,
        file_name,
        file_key_wrapped_blob,
        chunk_count,
        chunk_size,
        chunk_uuids_json,
        cloud_endpoint_json,
        expires_at,
        imported_at,
    ) = row;

    let sender_contact_id = sender_contact_id_text
        .map(|text| {
            let uuid =
                Uuid::parse_str(&text).map_err(|_| SharingError::InvalidContactId(text.clone()))?;
            Ok::<_, SharingError>(ContactId::from_uuid(uuid))
        })
        .transpose()?;

    let sender_public_key_bytes: [u8; 32] = sender_public_key_blob
        .try_into()
        .map_err(|blob: Vec<u8>| SharingError::InvalidPublicKeyLength(blob.len()))?;

    let file_key_wrapped: [u8; 72] = file_key_wrapped_blob
        .try_into()
        .map_err(|_| SharingError::Backend("file_key_wrapped is not 72 bytes".to_owned()))?;

    let chunk_uuids: Vec<String> = serde_json::from_str(&chunk_uuids_json)
        .map_err(|error| SharingError::InvalidJsonPayload(error.to_string()))?;

    let cloud_endpoint: serde_json::Value = serde_json::from_str(&cloud_endpoint_json)
        .map_err(|error| SharingError::InvalidJsonPayload(error.to_string()))?;

    Ok(ReceivedShare {
        share_id,
        sender_contact_id,
        sender_public_key: X25519PublicKey::new(sender_public_key_bytes),
        file_id,
        file_name,
        file_key_wrapped,
        chunk_count: chunk_count as u32,
        chunk_size: chunk_size as u32,
        chunk_uuids,
        cloud_endpoint,
        expires_at,
        imported_at,
    })
}

/// Converts storage-domain failures into sharing-domain errors for received shares.
fn map_received_share_error(error: StorageError) -> SharingError {
    match error {
        StorageError::NotFound => SharingError::ReceivedShareNotFound,
        StorageError::ConstraintViolation(message) => SharingError::ConstraintViolation(message),
        _ => SharingError::Backend("storage backend failure".to_owned()),
    }
}

/// Raw row tuple from a `shares` SELECT.
type ShareRow = (
    String,      // share_id
    String,      // file_id
    String,      // contact_id
    String,      // file_share_id
    String,      // cloud_path
    i64,         // created_at
    Option<i64>, // expires_at
    Option<i64>, // revoked_at
);

/// Maps a single rusqlite row into the raw `ShareRow` tuple.
fn map_share_row(row: &Row<'_>) -> rusqlite::Result<ShareRow> {
    Ok((
        row.get::<_, String>(0)?,
        row.get::<_, String>(1)?,
        row.get::<_, String>(2)?,
        row.get::<_, String>(3)?,
        row.get::<_, String>(4)?,
        row.get::<_, i64>(5)?,
        row.get::<_, Option<i64>>(6)?,
        row.get::<_, Option<i64>>(7)?,
    ))
}

/// Converts a raw `ShareRow` tuple into the domain `ShareRecord`.
fn parse_share_row(row: ShareRow) -> Result<ShareRecord, SharingError> {
    let (
        share_id,
        file_id,
        contact_id_text,
        file_share_id,
        cloud_path,
        created_at,
        expires_at,
        revoked_at,
    ) = row;
    let contact_uuid = Uuid::parse_str(&contact_id_text)
        .map_err(|_| SharingError::InvalidContactId(contact_id_text.clone()))?;
    Ok(ShareRecord {
        share_id,
        file_id,
        contact_id: ContactId::from_uuid(contact_uuid),
        file_share_id,
        cloud_path,
        created_at,
        expires_at,
        revoked_at,
    })
}

/// Converts storage-domain failures into sharing-domain errors for outgoing shares.
fn map_share_error(error: StorageError) -> SharingError {
    match error {
        StorageError::NotFound => SharingError::ShareNotFound,
        StorageError::ConstraintViolation(message) => SharingError::ConstraintViolation(message),
        _ => SharingError::Backend("storage backend failure".to_owned()),
    }
}

#[cfg(test)]
mod tests {
    use rusqlite::params;
    use tempfile::tempdir;
    use uuid::Uuid;

    use crate::sharing::{
        Contact, ContactId, DisplayName, ShareRecord, SharingError, SharingStore, X25519PublicKey,
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

    /// Creates a sample `ReceivedShare` for testing.
    fn sample_received_share(marker: u8) -> crate::sharing::ReceivedShare {
        crate::sharing::ReceivedShare {
            share_id: Uuid::new_v4().hyphenated().to_string(),
            sender_contact_id: None,
            sender_public_key: X25519PublicKey::new([marker; 32]),
            file_id: Uuid::new_v4().hyphenated().to_string(),
            file_name: format!("shared-doc-{marker}.pdf"),
            file_key_wrapped: [marker; 72],
            chunk_count: 2,
            chunk_size: 4_194_304,
            chunk_uuids: vec![
                Uuid::new_v4().hyphenated().to_string(),
                Uuid::new_v4().hyphenated().to_string(),
            ],
            cloud_endpoint: serde_json::json!({"provider": "s3", "bucket": "test"}),
            expires_at: Some(1_800_000_000),
            imported_at: 1_700_000_000 + marker as i64,
        }
    }

    /// Verifies insert → get → list round-trip for received shares.
    #[tokio::test]
    async fn test_sharing_store_received_share_crud_round_trip_preserves_fields() {
        let (_directory, store) = create_store().await;
        let share = sample_received_share(0x42);

        store
            .insert_received_share(&share)
            .await
            .expect("insert should succeed");

        let fetched = store
            .get_received_share(&share.share_id)
            .await
            .expect("get should succeed");
        assert_eq!(fetched.share_id, share.share_id);
        assert_eq!(fetched.sender_contact_id, None);
        assert_eq!(fetched.sender_public_key, share.sender_public_key);
        assert_eq!(fetched.file_name, share.file_name);
        assert_eq!(fetched.file_key_wrapped, share.file_key_wrapped);
        assert_eq!(fetched.chunk_count, share.chunk_count);
        assert_eq!(fetched.chunk_size, share.chunk_size);
        assert_eq!(fetched.chunk_uuids, share.chunk_uuids);
        assert_eq!(fetched.cloud_endpoint, share.cloud_endpoint);
        assert_eq!(fetched.expires_at, share.expires_at);
        assert_eq!(fetched.imported_at, share.imported_at);

        let listed = store
            .list_received_shares()
            .await
            .expect("list should succeed");
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].share_id, share.share_id);
    }

    /// Verifies duplicate share_id returns `ConstraintViolation`.
    #[tokio::test]
    async fn test_sharing_store_insert_received_share_duplicate_returns_constraint_violation() {
        let (_directory, store) = create_store().await;
        let share = sample_received_share(0x55);

        store
            .insert_received_share(&share)
            .await
            .expect("first insert should succeed");

        let result = store.insert_received_share(&share).await;
        assert!(
            matches!(result, Err(SharingError::ConstraintViolation(_))),
            "expected ConstraintViolation, got {result:?}"
        );
    }

    /// Verifies missing share_id returns `ReceivedShareNotFound`.
    #[tokio::test]
    async fn test_sharing_store_get_received_share_missing_returns_not_found() {
        let (_directory, store) = create_store().await;

        let result = store
            .get_received_share(&Uuid::new_v4().hyphenated().to_string())
            .await;
        assert!(matches!(result, Err(SharingError::ReceivedShareNotFound)));
    }

    /// Verifies `expires_at: None` round-trips to NULL in SQLCipher.
    #[tokio::test]
    async fn test_sharing_store_received_share_none_expires_at_round_trips() {
        let (_directory, store) = create_store().await;
        let mut share = sample_received_share(0x88);
        share.expires_at = None;

        store
            .insert_received_share(&share)
            .await
            .expect("insert should succeed");

        let fetched = store
            .get_received_share(&share.share_id)
            .await
            .expect("get should succeed");
        assert_eq!(fetched.expires_at, None);
    }

    /// Seeds a minimal `nodes` row required for outgoing-share foreign key constraints.
    async fn seed_node(store: &SqlCipherMetadataStore, node_id: &str) {
        let node_id_owned = node_id.to_owned();
        store
            .with_connection_blocking(move |connection| {
                connection
                    .execute(
                        "INSERT INTO nodes (node_id, parent_id, node_type, name, created_at, modified_at, size_bytes, file_key_wrapped) VALUES (?1, NULL, 'file', 'test.txt', 1000, 1000, 100, ?2)",
                        params![node_id_owned, vec![0xde_u8, 0xad, 0xbe, 0xef]],
                    )
                    .map_err(StorageError::from_rusqlite)?;
                Ok(())
            })
            .await
            .expect("node row insert should succeed");
    }

    /// Creates one outgoing `ShareRecord` test fixture with a unique share identifier.
    fn sample_share(file_id: &str, contact_id: ContactId, marker: u8) -> ShareRecord {
        let file_share_id = Uuid::new_v4().hyphenated().to_string();
        let cloud_path = format!("shared/{}/", file_share_id);
        ShareRecord {
            share_id: Uuid::new_v4().hyphenated().to_string(),
            file_id: file_id.to_owned(),
            contact_id,
            file_share_id,
            cloud_path,
            created_at: 1_700_000_000 + marker as i64,
            expires_at: None,
            revoked_at: None,
        }
    }

    /// Verifies insert → get round-trip preserves all `ShareRecord` fields.
    #[tokio::test]
    async fn test_sharing_store_share_crud_round_trip_preserves_all_fields() {
        let (_directory, store) = create_store().await;
        let contact = sample_contact("Alice", Some("alice@example.com"), 1);
        store
            .insert_contact(&contact)
            .await
            .expect("contact insert should succeed");
        let file_id = Uuid::new_v4().hyphenated().to_string();
        seed_node(&store, &file_id).await;
        let share = sample_share(&file_id, contact.contact_id, 0x01);

        store
            .insert_share(&share)
            .await
            .expect("insert should succeed");

        let fetched = store
            .get_share(&share.share_id)
            .await
            .expect("get should succeed");
        assert_eq!(fetched.share_id, share.share_id);
        assert_eq!(fetched.file_id, share.file_id);
        assert_eq!(fetched.contact_id, share.contact_id);
        assert_eq!(fetched.file_share_id, share.file_share_id);
        assert_eq!(fetched.cloud_path, share.cloud_path);
        assert_eq!(fetched.created_at, share.created_at);
        assert_eq!(fetched.expires_at, share.expires_at);
        assert_eq!(fetched.revoked_at, share.revoked_at);
    }

    /// Verifies `list_shares_by_file` returns only shares matching the given file identifier.
    #[tokio::test]
    async fn test_sharing_store_list_shares_by_file_returns_only_matching_file() {
        let (_directory, store) = create_store().await;
        let contact = sample_contact("Bob", None, 2);
        store
            .insert_contact(&contact)
            .await
            .expect("contact insert should succeed");
        let file_id_a = Uuid::new_v4().hyphenated().to_string();
        let file_id_b = Uuid::new_v4().hyphenated().to_string();
        seed_node(&store, &file_id_a).await;
        seed_node(&store, &file_id_b).await;
        let share_a = sample_share(&file_id_a, contact.contact_id, 0x01);
        let share_b = sample_share(&file_id_b, contact.contact_id, 0x02);
        store
            .insert_share(&share_a)
            .await
            .expect("insert share_a should succeed");
        store
            .insert_share(&share_b)
            .await
            .expect("insert share_b should succeed");

        let listed = store
            .list_shares_by_file(&file_id_a)
            .await
            .expect("list should succeed");
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].share_id, share_a.share_id);
    }

    /// Verifies `list_active_shares_by_file` excludes rows with a non-null `revoked_at`.
    #[tokio::test]
    async fn test_sharing_store_list_active_shares_by_file_excludes_revoked() {
        let (_directory, store) = create_store().await;
        let contact = sample_contact("Carol", None, 3);
        store
            .insert_contact(&contact)
            .await
            .expect("contact insert should succeed");
        let file_id = Uuid::new_v4().hyphenated().to_string();
        seed_node(&store, &file_id).await;
        let share_active = sample_share(&file_id, contact.contact_id, 0x01);
        let share_revoked = sample_share(&file_id, contact.contact_id, 0x02);
        store
            .insert_share(&share_active)
            .await
            .expect("insert active should succeed");
        store
            .insert_share(&share_revoked)
            .await
            .expect("insert revoked should succeed");
        store
            .set_share_revoked_at(&share_revoked.share_id, 1_800_000_000)
            .await
            .expect("revoke should succeed");

        let listed = store
            .list_active_shares_by_file(&file_id)
            .await
            .expect("list should succeed");
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].share_id, share_active.share_id);
    }

    /// Verifies `list_active_shares_by_file_share_id` filters by `file_share_id` and excludes revoked rows.
    #[tokio::test]
    async fn test_sharing_store_list_active_shares_by_file_share_id_returns_correct_shares() {
        let (_directory, store) = create_store().await;
        let contact = sample_contact("Dave", None, 4);
        store
            .insert_contact(&contact)
            .await
            .expect("contact insert should succeed");
        let file_id = Uuid::new_v4().hyphenated().to_string();
        seed_node(&store, &file_id).await;
        let target_file_share_id = Uuid::new_v4().hyphenated().to_string();
        let other_file_share_id = Uuid::new_v4().hyphenated().to_string();
        let mut share_1 = sample_share(&file_id, contact.contact_id, 0x01);
        share_1.file_share_id = target_file_share_id.clone();
        share_1.cloud_path = format!("shared/{}/", target_file_share_id);
        let mut share_2 = sample_share(&file_id, contact.contact_id, 0x02);
        share_2.file_share_id = target_file_share_id.clone();
        share_2.cloud_path = format!("shared/{}/", target_file_share_id);
        let mut share_3 = sample_share(&file_id, contact.contact_id, 0x03);
        share_3.file_share_id = other_file_share_id.clone();
        share_3.cloud_path = format!("shared/{}/", other_file_share_id);
        store
            .insert_share(&share_1)
            .await
            .expect("insert share_1 should succeed");
        store
            .insert_share(&share_2)
            .await
            .expect("insert share_2 should succeed");
        store
            .insert_share(&share_3)
            .await
            .expect("insert share_3 should succeed");

        let listed = store
            .list_active_shares_by_file_share_id(&target_file_share_id)
            .await
            .expect("list should succeed");
        assert_eq!(listed.len(), 2);
        let returned_ids: std::collections::HashSet<&str> =
            listed.iter().map(|share| share.share_id.as_str()).collect();
        assert!(returned_ids.contains(share_1.share_id.as_str()));
        assert!(returned_ids.contains(share_2.share_id.as_str()));
    }

    /// Verifies `set_share_revoked_at` on a missing share identifier returns `ShareNotFound`.
    #[tokio::test]
    async fn test_sharing_store_set_share_revoked_at_on_missing_share_returns_share_not_found() {
        let (_directory, store) = create_store().await;

        let result = store
            .set_share_revoked_at(&Uuid::new_v4().hyphenated().to_string(), 1_800_000_000)
            .await;
        assert!(matches!(result, Err(SharingError::ShareNotFound)));
    }

    /// Verifies `set_share_revoked_at` on an already-revoked share returns `ShareAlreadyRevoked`.
    #[tokio::test]
    async fn test_sharing_store_set_share_revoked_at_on_already_revoked_share_returns_share_already_revoked()
     {
        let (_directory, store) = create_store().await;
        let contact = sample_contact("Eve", None, 5);
        store
            .insert_contact(&contact)
            .await
            .expect("contact insert should succeed");
        let file_id = Uuid::new_v4().hyphenated().to_string();
        seed_node(&store, &file_id).await;
        let share = sample_share(&file_id, contact.contact_id, 0x01);
        store
            .insert_share(&share)
            .await
            .expect("insert should succeed");
        store
            .set_share_revoked_at(&share.share_id, 1_800_000_000)
            .await
            .expect("first revoke should succeed");

        let result = store
            .set_share_revoked_at(&share.share_id, 1_900_000_000)
            .await;
        assert!(matches!(result, Err(SharingError::ShareAlreadyRevoked)));
    }
}
