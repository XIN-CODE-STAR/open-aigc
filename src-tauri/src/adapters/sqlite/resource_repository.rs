use std::path::Path;

use rusqlite::{params, params_from_iter, Connection, OptionalExtension, TransactionBehavior};
use time::{format_description::well_known::Rfc3339, OffsetDateTime};
use uuid::Uuid;

use crate::{
    domain::resources::{
        AssociationContextKind, AssociationDraft, AssociationFilter, AssociationRecord,
        AssociationRole,
    },
    ports::{
        persistence::PersistenceError,
        resource_repository::{ResourceRepository, ResourceRepositoryError},
    },
};

pub struct SqliteResourceRepository {
    connection: Connection,
}

impl SqliteResourceRepository {
    pub fn open(database_path: impl AsRef<Path>) -> Result<Self, PersistenceError> {
        use crate::adapters::sqlite::database::open_database;
        let connection = open_database(database_path.as_ref())?;
        Ok(Self { connection })
    }
}

impl ResourceRepository for SqliteResourceRepository {
    fn list(
        &mut self,
        filter: &AssociationFilter,
    ) -> Result<Vec<AssociationRecord>, ResourceRepositoryError> {
        let mut query = String::from(
            r#"
            SELECT
              id,
              asset_id,
              context_kind,
              context_ref,
              role,
              notes,
              revision,
              created_at,
              updated_at
            FROM resource_association
            WHERE deleted_at IS NULL
            "#,
        );

        let mut params_vec: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();
        let mut param_index = 1;
        if let Some(asset_id) = filter.asset_id.as_deref() {
            query.push_str(&format!(" AND asset_id = ?{param_index}"));
            params_vec.push(Box::new(asset_id.to_owned()));
            param_index += 1;
        }
        if let Some(kind) = filter.context_kind {
            query.push_str(&format!(" AND context_kind = ?{param_index}"));
            params_vec.push(Box::new(kind.as_str().to_owned()));
            param_index += 1;
        }
        if let Some(context_ref) = filter.context_ref.as_deref() {
            query.push_str(&format!(" AND context_ref = ?{param_index}"));
            params_vec.push(Box::new(context_ref.to_owned()));
            param_index += 1;
        }
        if let Some(role) = filter.role {
            query.push_str(&format!(" AND role = ?{param_index}"));
            params_vec.push(Box::new(role.as_str().to_owned()));
            param_index += 1;
        }
        query.push_str(&format!(
            " ORDER BY updated_at DESC, created_at DESC LIMIT ?{param_index}"
        ));
        params_vec.push(Box::new(filter.limit));

        let mut statement = self
            .connection
            .prepare(&query)
            .map_err(|error| PersistenceError::new("prepare resource list", error))?;
        let params_iter = params_from_iter(params_vec.iter().map(|b| b.as_ref()));
        let rows = statement
            .query_map(params_iter, map_association_record)
            .map_err(|error| PersistenceError::new("query resource list", error))?;
        let mut records = Vec::new();
        for row in rows {
            records.push(row.map_err(|error| PersistenceError::new("read resource row", error))?);
        }
        Ok(records)
    }

    fn get(
        &mut self,
        association_id: &str,
    ) -> Result<Option<AssociationRecord>, ResourceRepositoryError> {
        let record = self
            .connection
            .query_row(
                r#"
                SELECT
                  id,
                  asset_id,
                  context_kind,
                  context_ref,
                  role,
                  notes,
                  revision,
                  created_at,
                  updated_at
                FROM resource_association
                WHERE deleted_at IS NULL AND id = ?1
                "#,
                params![association_id],
                map_association_record,
            )
            .optional()
            .map_err(|error| PersistenceError::new("read resource association", error))?;
        Ok(record)
    }

    fn create(
        &mut self,
        draft: AssociationDraft,
    ) -> Result<AssociationRecord, ResourceRepositoryError> {
        // 校验 asset 存在。
        let exists: bool = self
            .connection
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM asset_manifest WHERE id = ?1 AND deleted_at IS NULL)",
                params![draft.asset_id],
                |row| row.get::<_, i64>(0),
            )
            .map(|exists| exists == 1)
            .map_err(|error| PersistenceError::new("check asset exists for association", error))?;
        if !exists {
            return Err(ResourceRepositoryError::AssetNotFound(
                draft.asset_id.clone(),
            ));
        }

        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|error| PersistenceError::new("begin association transaction", error))?;
        let id = Uuid::new_v4().to_string();
        let now = OffsetDateTime::now_utc()
            .format(&Rfc3339)
            .map_err(|error| PersistenceError::new("format association timestamp", error))?;

        let result = transaction.execute(
            r#"
            INSERT INTO resource_association (
              id, asset_id, context_kind, context_ref, role, notes, revision, created_at, updated_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, 1, ?7, ?7)
            "#,
            params![
                id,
                draft.asset_id,
                draft.context_kind.as_str(),
                draft.context_ref,
                draft.role.as_str(),
                draft.notes,
                now
            ],
        );

        if let Err(error) = result {
            if is_unique_violation(&error) {
                return Err(ResourceRepositoryError::Duplicate {
                    asset_id: draft.asset_id.clone(),
                });
            }
            return Err(ResourceRepositoryError::Persistence(PersistenceError::new(
                "insert resource association",
                error,
            )));
        }

        transaction
            .commit()
            .map_err(|error| PersistenceError::new("commit association", error))?;

        Ok(AssociationRecord {
            id,
            asset_id: draft.asset_id,
            context_kind: draft.context_kind,
            context_ref: draft.context_ref,
            role: draft.role,
            notes: draft.notes,
            revision: 1,
            created_at: now.clone(),
            updated_at: now,
        })
    }

    fn delete(&mut self, association_id: &str) -> Result<(), ResourceRepositoryError> {
        let now = OffsetDateTime::now_utc()
            .format(&Rfc3339)
            .map_err(|error| PersistenceError::new("format association delete timestamp", error))?;
        let affected = self
            .connection
            .execute(
                "UPDATE resource_association SET deleted_at = ?1, updated_at = ?1 WHERE id = ?2 AND deleted_at IS NULL",
                params![now, association_id],
            )
            .map_err(|error| PersistenceError::new("delete resource association", error))?;
        if affected == 0 {
            return Err(ResourceRepositoryError::AssetNotFound(
                association_id.to_owned(),
            ));
        }
        Ok(())
    }
}

fn is_unique_violation(error: &rusqlite::Error) -> bool {
    match error {
        rusqlite::Error::SqliteFailure(err, _) => {
            err.code == rusqlite::ErrorCode::ConstraintViolation
        }
        _ => false,
    }
}

fn map_association_record(row: &rusqlite::Row<'_>) -> rusqlite::Result<AssociationRecord> {
    let context_kind_str: String = row.get(2)?;
    let role_str: String = row.get(4)?;
    let context_kind = AssociationContextKind::parse(&context_kind_str).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(2, rusqlite::types::Type::Text, Box::new(error))
    })?;
    let role = AssociationRole::parse(&role_str).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(4, rusqlite::types::Type::Text, Box::new(error))
    })?;
    Ok(AssociationRecord {
        id: row.get(0)?,
        asset_id: row.get(1)?,
        context_kind,
        context_ref: row.get(3)?,
        role,
        notes: row.get(5)?,
        revision: row.get(6)?,
        created_at: row.get(7)?,
        updated_at: row.get(8)?,
    })
}

#[cfg(test)]
mod tests {
    use tempfile::tempdir;

    use super::*;
    use crate::adapters::sqlite::asset_repository::{
        begin_import_tx, insert_asset_in_tx, now_rfc3339, SqliteAssetRepository,
    };
    use crate::adapters::sqlite::workspace_repository::SqliteWorkspaceRepository;
    use crate::domain::assets::{AssetDraft, AssetKind};
    use crate::domain::resources::{AssociationContextKind, AssociationDraft, AssociationFilter};
    use crate::domain::workspace::NewWorkspace;
    use crate::ports::resource_repository::ResourceRepository;
    use crate::ports::workspace_repository::WorkspaceRepository;

    const SAMPLE_ASSET_UUID: &str = "123e4567-e89b-42d3-a456-426614174000";

    fn seed() -> (tempfile::TempDir, SqliteResourceRepository, String) {
        let directory = tempdir().unwrap();
        let path = directory.path().join("workspace.sqlite3");
        let mut workspace = SqliteWorkspaceRepository::open(&path).unwrap();
        workspace
            .initialize(&NewWorkspace::try_new("测试工作空间", "测试教师").unwrap())
            .unwrap();
        drop(workspace);

        // 通过 asset 仓库插入一条真实 manifest 记录。
        let mut asset_repository = SqliteAssetRepository::open(&path).unwrap();
        let created_at = now_rfc3339().unwrap();
        let draft = AssetDraft::try_new(
            "workspace".to_owned(),
            AssetKind::Image,
            "示例".to_owned(),
            "assets/sample.png".to_owned(),
            4,
            None,
            None,
            None,
        )
        .unwrap();
        let tx = begin_import_tx(&mut asset_repository.connection).unwrap();
        let record = insert_asset_in_tx(&tx, &draft, None, &created_at).unwrap();
        tx.commit().unwrap();
        let asset_id = record.id;
        drop(asset_repository);

        let repository = SqliteResourceRepository::open(&path).unwrap();
        (directory, repository, asset_id)
    }

    #[test]
    fn creates_and_reads_back_association() {
        let (_directory, mut repository, asset_id) = seed();
        let draft = AssociationDraft::try_new(
            asset_id.clone(),
            "teaching-resource".to_owned(),
            "classroom:abc".to_owned(),
            "source".to_owned(),
            Some("教案原图".to_owned()),
        )
        .unwrap();

        let created = repository.create(draft).unwrap();
        assert_eq!(created.asset_id, asset_id);
        assert_eq!(
            created.context_kind,
            AssociationContextKind::TeachingResource
        );

        let fetched = repository.get(&created.id).unwrap().unwrap();
        assert_eq!(fetched, created);
    }

    #[test]
    fn rejects_association_for_missing_asset() {
        let (_directory, mut repository, _asset_id) = seed();
        let draft = AssociationDraft::try_new(
            SAMPLE_ASSET_UUID.to_owned(),
            "teaching-resource".to_owned(),
            String::new(),
            "source".to_owned(),
            None,
        )
        .unwrap();

        let error = repository.create(draft).unwrap_err();
        assert!(matches!(error, ResourceRepositoryError::AssetNotFound(_)));
    }

    #[test]
    fn rejects_duplicate_association() {
        let (_directory, mut repository, asset_id) = seed();
        let draft = AssociationDraft::try_new(
            asset_id.clone(),
            "teaching-resource".to_owned(),
            "classroom:abc".to_owned(),
            "source".to_owned(),
            None,
        )
        .unwrap();
        repository.create(draft.clone()).unwrap();

        let error = repository.create(draft).unwrap_err();
        assert!(matches!(
            error,
            ResourceRepositoryError::Duplicate { asset_id: _ }
        ));
    }

    #[test]
    fn list_filters_by_asset() {
        let (_directory, mut repository, asset_id) = seed();
        let draft = AssociationDraft::try_new(
            asset_id.clone(),
            "teaching-resource".to_owned(),
            "classroom:abc".to_owned(),
            "source".to_owned(),
            None,
        )
        .unwrap();
        repository.create(draft).unwrap();

        let filter = AssociationFilter::try_new(Some(asset_id), None, None, None, None).unwrap();
        let records = repository.list(&filter).unwrap();
        assert_eq!(records.len(), 1);
    }

    #[test]
    fn delete_soft_deletes_association() {
        let (_directory, mut repository, asset_id) = seed();
        let draft = AssociationDraft::try_new(
            asset_id,
            "teaching-resource".to_owned(),
            String::new(),
            "reference".to_owned(),
            None,
        )
        .unwrap();
        let created = repository.create(draft).unwrap();
        repository.delete(&created.id).unwrap();

        let filter = AssociationFilter::try_new(None, None, None, None, None).unwrap();
        assert!(repository.list(&filter).unwrap().is_empty());
        assert!(repository.get(&created.id).unwrap().is_none());
    }
}
