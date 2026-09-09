use std::path::Path;

use rusqlite::{params, OptionalExtension, TransactionBehavior};
use time::{format_description::well_known::Rfc3339, OffsetDateTime};
use uuid::Uuid;

use crate::{
    adapters::file_vault::FileVault,
    domain::credentials::{CredentialDraft, CredentialRecord},
    ports::{
        credential_repository::{CredentialRepository, CredentialRepositoryError},
        persistence::PersistenceError,
    },
};

pub struct SqliteCredentialRepository {
    connection: rusqlite::Connection,
    vault: FileVault,
}

impl SqliteCredentialRepository {
    pub fn open(database_path: impl AsRef<Path>) -> Result<Self, PersistenceError> {
        use crate::adapters::sqlite::database::open_database;
        let path = database_path.as_ref();
        let connection = open_database(path)?;
        let vault = FileVault::from_database_path(path)
            .map_err(|e| PersistenceError::new("open secret vault", e))?;
        Ok(Self { connection, vault })
    }
}

impl CredentialRepository for SqliteCredentialRepository {
    fn list(&mut self) -> Result<Vec<CredentialRecord>, CredentialRepositoryError> {
        let mut statement = self
            .connection
            .prepare(
                r#"
                SELECT id, provider_name, display_name, base_url, model_name,
                       credential_key, enabled, created_at, updated_at,
                       credential_type, scope
                FROM provider_credentials
                WHERE deleted_at IS NULL
                ORDER BY created_at ASC
                "#,
            )
            .map_err(|e| PersistenceError::new("prepare credential list", e))?;
        let rows = statement
            .query_map([], map_credential_record)
            .map_err(|e| PersistenceError::new("query credential list", e))?;
        let mut records = Vec::new();
        for row in rows {
            records.push(row.map_err(|e| PersistenceError::new("read credential row", e))?);
        }
        Ok(records)
    }

    fn get(&mut self, id: &str) -> Result<Option<CredentialRecord>, CredentialRepositoryError> {
        let record = self
            .connection
            .query_row(
                r#"SELECT id, provider_name, display_name, base_url, model_name,
                          credential_key, enabled, created_at, updated_at,
                          credential_type, scope
                   FROM provider_credentials
                   WHERE deleted_at IS NULL AND id = ?1"#,
                params![id],
                map_credential_record,
            )
            .optional()
            .map_err(|e| PersistenceError::new("read credential", e))?;
        Ok(record)
    }

    fn create(
        &mut self,
        draft: CredentialDraft,
        secret: String,
    ) -> Result<CredentialRecord, CredentialRepositoryError> {
        let id = Uuid::new_v4().to_string();
        let credential_key = format!("credential:{id}");
        let now = now_rfc3339()?;

        // 先写入加密保险库，失败则不创建数据库记录。
        self.vault
            .set_secret(&credential_key, &secret)
            .map_err(|e| CredentialRepositoryError::Keychain(e.to_string()))?;

        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|e| PersistenceError::new("begin credential transaction", e))?;
        tx.execute(
            r#"INSERT INTO provider_credentials
               (id, provider_name, display_name, base_url, model_name,
                credential_key, enabled, created_at, updated_at,
                credential_type, scope)
               VALUES (?1, ?2, ?3, ?4, ?5, ?6, 1, ?7, ?7, ?8, 'user')"#,
            params![
                id,
                draft.provider_name,
                draft.display_name,
                draft.base_url,
                draft.model_name,
                credential_key,
                now,
                draft.credential_type.as_str()
            ],
        )
        .map_err(|e| PersistenceError::new("insert credential", e))?;
        tx.commit()
            .map_err(|e| PersistenceError::new("commit credential", e))?;

        Ok(CredentialRecord {
            id,
            provider_name: draft.provider_name,
            display_name: draft.display_name,
            base_url: draft.base_url,
            model_name: draft.model_name,
            credential_key,
            enabled: true,
            credential_type: draft.credential_type,
            scope: crate::domain::credentials::CredentialScope::User,
            created_at: now.clone(),
            updated_at: now,
        })
    }

    fn update(
        &mut self,
        id: &str,
        draft: CredentialDraft,
        secret: Option<String>,
    ) -> Result<CredentialRecord, CredentialRepositoryError> {
        let existing = self
            .get(id)?
            .ok_or_else(|| CredentialRepositoryError::NotFound(id.to_owned()))?;

        if let Some(secret) = secret {
            self.vault
                .set_secret(&existing.credential_key, &secret)
                .map_err(|e| CredentialRepositoryError::Keychain(e.to_string()))?;
        }

        let now = now_rfc3339()?;
        let affected = self
            .connection
            .execute(
                r#"UPDATE provider_credentials
                   SET provider_name = ?2, display_name = ?3, base_url = ?4,
                       model_name = ?5, updated_at = ?6
                   WHERE id = ?1 AND deleted_at IS NULL"#,
                params![
                    id,
                    draft.provider_name,
                    draft.display_name,
                    draft.base_url,
                    draft.model_name,
                    now
                ],
            )
            .map_err(|e| PersistenceError::new("update credential", e))?;
        if affected == 0 {
            return Err(CredentialRepositoryError::NotFound(id.to_owned()));
        }

        Ok(CredentialRecord {
            id: id.to_owned(),
            provider_name: draft.provider_name,
            display_name: draft.display_name,
            base_url: draft.base_url,
            model_name: draft.model_name,
            credential_key: existing.credential_key,
            enabled: existing.enabled,
            credential_type: existing.credential_type,
            scope: existing.scope,
            created_at: existing.created_at,
            updated_at: now,
        })
    }

    fn delete(&mut self, id: &str) -> Result<(), CredentialRepositoryError> {
        let existing = self
            .get(id)?
            .ok_or_else(|| CredentialRepositoryError::NotFound(id.to_owned()))?;
        let now = now_rfc3339()?;
        let affected = self
            .connection
            .execute(
                "UPDATE provider_credentials SET deleted_at = ?1, updated_at = ?1 WHERE id = ?2 AND deleted_at IS NULL",
                params![now, id],
            )
            .map_err(|e| PersistenceError::new("delete credential", e))?;
        if affected == 0 {
            return Err(CredentialRepositoryError::NotFound(id.to_owned()));
        }
        // 尝试从保险库删除密钥，失败不阻止软删。
        let _ = self.vault.delete_secret(&existing.credential_key);
        Ok(())
    }

    fn get_secret(&mut self, credential_key: &str) -> Result<String, CredentialRepositoryError> {
        self.vault
            .get_secret(credential_key)
            .map_err(|e| CredentialRepositoryError::Keychain(e.to_string()))
    }
}

fn now_rfc3339() -> Result<String, CredentialRepositoryError> {
    let formatted = OffsetDateTime::now_utc()
        .format(&Rfc3339)
        .map_err(|e| PersistenceError::new("format credential timestamp", e))?;
    Ok(formatted)
}

fn map_credential_record(row: &rusqlite::Row<'_>) -> rusqlite::Result<CredentialRecord> {
    use crate::domain::credentials::{CredentialScope, CredentialType};
    let credential_type_str: String = row.get(9)?;
    let scope_str: String = row.get(10)?;
    Ok(CredentialRecord {
        id: row.get(0)?,
        provider_name: row.get(1)?,
        display_name: row.get(2)?,
        base_url: row.get(3)?,
        model_name: row.get(4)?,
        credential_key: row.get(5)?,
        enabled: row.get::<_, i64>(6)? == 1,
        credential_type: CredentialType::parse(&credential_type_str),
        scope: CredentialScope::parse(&scope_str),
        created_at: row.get(7)?,
        updated_at: row.get(8)?,
    })
}

#[cfg(test)]
mod tests {
    use tempfile::tempdir;

    use super::*;
    use crate::adapters::sqlite::workspace_repository::SqliteWorkspaceRepository;
    use crate::domain::workspace::NewWorkspace;
    use crate::ports::workspace_repository::WorkspaceRepository;

    fn seed() -> (tempfile::TempDir, SqliteCredentialRepository) {
        let directory = tempdir().unwrap();
        let path = directory.path().join("workspace.sqlite3");
        let mut workspace = SqliteWorkspaceRepository::open(&path).unwrap();
        workspace
            .initialize(&NewWorkspace::try_new("测试", "测试教师").unwrap())
            .unwrap();
        drop(workspace);
        let repository = SqliteCredentialRepository::open(&path).unwrap();
        (directory, repository)
    }

    fn sample_draft() -> CredentialDraft {
        CredentialDraft::try_new(
            "Seedance".to_owned(),
            "种子舞蹈".to_owned(),
            "https://api.seedance.com".to_owned(),
            "seedance-v2".to_owned(),
        )
        .unwrap()
    }

    #[test]
    fn creates_and_lists_credential() {
        let (_dir, mut repo) = seed();
        let record = repo
            .create(sample_draft(), "sk-test-key".to_owned())
            .unwrap();
        assert_eq!(record.provider_name, "Seedance");
        assert!(record.enabled);

        let list = repo.list().unwrap();
        assert_eq!(list.len(), 1);
    }

    #[test]
    fn updates_credential_metadata() {
        let (_dir, mut repo) = seed();
        let record = repo
            .create(sample_draft(), "sk-test-key".to_owned())
            .unwrap();
        let updated = repo
            .update(
                &record.id,
                CredentialDraft::try_new(
                    "Kling".to_owned(),
                    "可灵".to_owned(),
                    "https://api.kling.com".to_owned(),
                    "kling-v1".to_owned(),
                )
                .unwrap(),
                None,
            )
            .unwrap();
        assert_eq!(updated.provider_name, "Kling");
        assert_eq!(updated.model_name, "kling-v1");
    }

    #[test]
    fn deletes_credential() {
        let (_dir, mut repo) = seed();
        let record = repo
            .create(sample_draft(), "sk-test-key".to_owned())
            .unwrap();
        repo.delete(&record.id).unwrap();
        assert!(repo.get(&record.id).unwrap().is_none());
    }

    #[test]
    fn rejects_update_for_missing_credential() {
        let (_dir, mut repo) = seed();
        let error = repo
            .update("nonexistent", sample_draft(), None)
            .unwrap_err();
        assert!(matches!(error, CredentialRepositoryError::NotFound(_)));
    }
}
