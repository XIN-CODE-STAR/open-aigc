use std::path::Path;

use rusqlite::{params, OptionalExtension};
use uuid::Uuid;

use crate::{
    adapters::file_vault::FileVault,
    ports::{
        persistence::PersistenceError,
        resource_account_repository::{
            ResourceAccountDraft, ResourceAccountRepository, ResourceAccountRepositoryError,
            ResourceAccountUpdate,
        },
        resource_connector::ResourceAccountRecord,
    },
};

pub struct SqliteResourceAccountRepository {
    connection: rusqlite::Connection,
    vault: FileVault,
}

impl SqliteResourceAccountRepository {
    pub fn open(database_path: impl AsRef<Path>) -> Result<Self, PersistenceError> {
        use crate::adapters::sqlite::database::open_database;
        let path = database_path.as_ref();
        let connection = open_database(path)?;
        let vault = FileVault::from_database_path(path)
            .map_err(|e| PersistenceError::new("open secret vault", e))?;
        Ok(Self { connection, vault })
    }
}

impl ResourceAccountRepository for SqliteResourceAccountRepository {
    fn list(&mut self) -> Result<Vec<ResourceAccountRecord>, ResourceAccountRepositoryError> {
        let mut stmt = self
            .connection
            .prepare(
                r#"SELECT id, provider_id, account_type, display_name, status,
                          credential_key, base_url, extra_json, enabled,
                          created_at, updated_at, last_health_check_at
                   FROM resource_accounts
                   WHERE deleted_at IS NULL
                   ORDER BY created_at ASC"#,
            )
            .map_err(|e| PersistenceError::new("prepare resource account list", e))?;
        let rows = stmt
            .query_map([], map_account_record)
            .map_err(|e| PersistenceError::new("query resource account list", e))?;
        let mut records = Vec::new();
        for row in rows {
            records.push(row.map_err(|e| PersistenceError::new("read resource account row", e))?);
        }
        Ok(records)
    }

    fn get(
        &mut self,
        id: &str,
    ) -> Result<Option<ResourceAccountRecord>, ResourceAccountRepositoryError> {
        let record = self
            .connection
            .query_row(
                r#"SELECT id, provider_id, account_type, display_name, status,
                          credential_key, base_url, extra_json, enabled,
                          created_at, updated_at, last_health_check_at
                   FROM resource_accounts
                   WHERE deleted_at IS NULL AND id = ?1"#,
                params![id],
                map_account_record,
            )
            .optional()
            .map_err(|e| PersistenceError::new("read resource account", e))?;
        Ok(record)
    }

    fn list_by_provider(
        &mut self,
        provider_id: &str,
    ) -> Result<Vec<ResourceAccountRecord>, ResourceAccountRepositoryError> {
        let mut stmt = self
            .connection
            .prepare(
                r#"SELECT id, provider_id, account_type, display_name, status,
                          credential_key, base_url, extra_json, enabled,
                          created_at, updated_at, last_health_check_at
                   FROM resource_accounts
                   WHERE deleted_at IS NULL AND provider_id = ?1 AND enabled = 1 AND status = 'active'
                   ORDER BY created_at ASC"#,
            )
            .map_err(|e| PersistenceError::new("prepare list_by_provider", e))?;
        let rows = stmt
            .query_map(params![provider_id], map_account_record)
            .map_err(|e| PersistenceError::new("query list_by_provider", e))?;
        let mut records = Vec::new();
        for row in rows {
            records.push(row.map_err(|e| PersistenceError::new("read account row", e))?);
        }
        Ok(records)
    }

    fn create(
        &mut self,
        draft: ResourceAccountDraft,
        session_secret: String,
    ) -> Result<ResourceAccountRecord, ResourceAccountRepositoryError> {
        let id = Uuid::new_v4().to_string();
        let credential_key = format!("resource_account:{id}");
        let now = crate::adapters::sqlite::now_rfc3339()?;

        // 先写入加密保险库
        self.vault
            .set_secret(&credential_key, &session_secret)
            .map_err(|e| ResourceAccountRepositoryError::Keychain(e.to_string()))?;

        self.connection
            .execute(
                r#"INSERT INTO resource_accounts
                   (id, provider_id, account_type, display_name, status,
                    credential_key, base_url, extra_json, enabled,
                    created_at, updated_at)
                   VALUES (?1, ?2, ?3, ?4, 'active', ?5, ?6, ?7, 1, ?8, ?8)"#,
                params![
                    id,
                    draft.provider_id,
                    draft.account_type,
                    draft.display_name,
                    credential_key,
                    draft.base_url,
                    draft.extra_json,
                    now
                ],
            )
            .map_err(|e| PersistenceError::new("insert resource account", e))?;

        Ok(ResourceAccountRecord {
            id,
            provider_id: draft.provider_id,
            account_type: draft.account_type,
            display_name: draft.display_name,
            status: "active".to_owned(),
            credential_key,
            base_url: draft.base_url,
            extra_json: draft.extra_json,
            enabled: true,
            created_at: now.clone(),
            updated_at: now,
            last_health_check_at: None,
        })
    }

    fn update(
        &mut self,
        id: &str,
        update: ResourceAccountUpdate,
    ) -> Result<ResourceAccountRecord, ResourceAccountRepositoryError> {
        let existing = self
            .get(id)?
            .ok_or_else(|| ResourceAccountRepositoryError::NotFound(id.to_owned()))?;

        let now = crate::adapters::sqlite::now_rfc3339()?;
        let affected = self
            .connection
            .execute(
                r#"UPDATE resource_accounts
                   SET display_name = ?2, base_url = ?3, extra_json = ?4, updated_at = ?5
                   WHERE id = ?1 AND deleted_at IS NULL"#,
                params![
                    id,
                    update.display_name,
                    update.base_url,
                    update.extra_json,
                    now
                ],
            )
            .map_err(|e| PersistenceError::new("update resource account", e))?;
        if affected == 0 {
            return Err(ResourceAccountRepositoryError::NotFound(id.to_owned()));
        }

        Ok(ResourceAccountRecord {
            id: id.to_owned(),
            display_name: update.display_name,
            base_url: update.base_url,
            extra_json: update.extra_json,
            updated_at: now,
            ..existing
        })
    }

    fn update_session(
        &mut self,
        id: &str,
        session_secret: String,
    ) -> Result<(), ResourceAccountRepositoryError> {
        let existing = self
            .get(id)?
            .ok_or_else(|| ResourceAccountRepositoryError::NotFound(id.to_owned()))?;

        self.vault
            .set_secret(&existing.credential_key, &session_secret)
            .map_err(|e| ResourceAccountRepositoryError::Keychain(e.to_string()))?;

        let now = crate::adapters::sqlite::now_rfc3339()?;
        self.connection
            .execute(
                r#"UPDATE resource_accounts SET status = 'active', updated_at = ?2 WHERE id = ?1"#,
                params![id, now],
            )
            .map_err(|e| PersistenceError::new("update session status", e))?;
        Ok(())
    }

    fn update_status(
        &mut self,
        id: &str,
        status: &str,
    ) -> Result<(), ResourceAccountRepositoryError> {
        let now = crate::adapters::sqlite::now_rfc3339()?;
        let affected = self
            .connection
            .execute(
                r#"UPDATE resource_accounts
                   SET status = ?2, last_health_check_at = ?3, updated_at = ?3
                   WHERE id = ?1 AND deleted_at IS NULL"#,
                params![id, status, now],
            )
            .map_err(|e| PersistenceError::new("update account status", e))?;
        if affected == 0 {
            return Err(ResourceAccountRepositoryError::NotFound(id.to_owned()));
        }
        Ok(())
    }

    fn set_enabled(
        &mut self,
        id: &str,
        enabled: bool,
    ) -> Result<(), ResourceAccountRepositoryError> {
        let now = crate::adapters::sqlite::now_rfc3339()?;
        let affected = self
            .connection
            .execute(
                r#"UPDATE resource_accounts SET enabled = ?2, updated_at = ?3 WHERE id = ?1 AND deleted_at IS NULL"#,
                params![id, enabled as i32, now],
            )
            .map_err(|e| PersistenceError::new("set account enabled", e))?;
        if affected == 0 {
            return Err(ResourceAccountRepositoryError::NotFound(id.to_owned()));
        }
        Ok(())
    }

    fn delete(&mut self, id: &str) -> Result<(), ResourceAccountRepositoryError> {
        let existing = self
            .get(id)?
            .ok_or_else(|| ResourceAccountRepositoryError::NotFound(id.to_owned()))?;

        // 清理保险库
        let _ = self.vault.delete_secret(&existing.credential_key);

        let now = crate::adapters::sqlite::now_rfc3339()?;
        self.connection
            .execute(
                r#"UPDATE resource_accounts SET deleted_at = ?2 WHERE id = ?1"#,
                params![id, now],
            )
            .map_err(|e| PersistenceError::new("soft delete resource account", e))?;
        Ok(())
    }

    fn get_session_secret(
        &mut self,
        credential_key: &str,
    ) -> Result<String, ResourceAccountRepositoryError> {
        self.vault
            .get_secret(credential_key)
            .map_err(|e| ResourceAccountRepositoryError::Keychain(e.to_string()))
    }
}

// ── Row mapper ──

fn map_account_record(row: &rusqlite::Row<'_>) -> rusqlite::Result<ResourceAccountRecord> {
    Ok(ResourceAccountRecord {
        id: row.get(0)?,
        provider_id: row.get(1)?,
        account_type: row.get(2)?,
        display_name: row.get(3)?,
        status: row.get(4)?,
        credential_key: row.get(5)?,
        base_url: row.get(6)?,
        extra_json: row.get(7)?,
        enabled: row.get::<_, i32>(8)? != 0,
        created_at: row.get(9)?,
        updated_at: row.get(10)?,
        last_health_check_at: row.get(11)?,
    })
}
