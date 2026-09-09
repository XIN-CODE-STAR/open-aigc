#![allow(dead_code)]
//! Artifact Importer：Creative Runtime 资产导入器。
//!
//! 将 PollWorker 下载的原始文件转化为可追踪的资产记录：
//! DownloadedFile → AssetService.import()（SHA256 + staging + 去重 + AssetRecord）→ ImportedArtifact
//!
//! 复用现有 AssetService 基础设施，不重建轮子。
//!
//! 职责链位置：
//! PollWorker（下载）→ ArtifactImporter（导入）→ Critic / Content Guard（后续步骤）

use std::collections::HashMap;
use std::sync::Arc;

use crate::application::asset_service::AssetService;
use crate::domain::creative_plan::AssetType;

// ─── ImportInput ───

/// 导入输入：描述一个待导入的资产文件及其创作上下文。
#[derive(Debug, Clone)]
pub struct ImportInput {
    /// PollWorker 下载到的本地文件路径。
    pub file_path: String,
    /// 资产显示名称（用于 AssetRecord.display_name）。
    pub display_name: String,
    /// 存储命名空间（"workspace" / "generation"）。
    pub namespace: String,
    /// 原始 AssetType（用于 metadata 追溯）。
    pub asset_type: AssetType,
    /// 关联的 submission_id（追溯 Facade 提交）。
    pub submission_id: Option<String>,
    /// 使用的 Provider 标识。
    pub provider_id: String,
    /// Router 选中的模型名。
    pub model: String,
    /// 执行步骤索引。
    pub step_index: u8,
    /// 镜头索引。
    pub shot_index: u8,
    /// 来源运行时 Artifact ID（追溯 Execution Artifact → Imported Asset 链路）。
    pub source_artifact_id: Option<String>,
}

// ─── ImportedArtifact ───

/// 导入成功的资产记录。
#[derive(Debug, Clone)]
pub struct ImportedArtifact {
    /// AssetRecord.id（SQLite 主键）。
    pub asset_id: String,
    /// 资产在 managed-files/ 下的相对路径。
    pub relative_path: String,
    /// SHA-256 hash（64 位小写 hex）。
    pub sha256: Option<String>,
    /// 文件大小（字节）。
    pub size_bytes: i64,
    /// MIME 类型。
    pub mime_type: Option<String>,
    /// 资产版本号（AssetRecord.revision，初始为 1）。
    pub revision: i64,
    /// 创作上下文 metadata（provider、model、step_index 等）。
    pub metadata: HashMap<String, serde_json::Value>,
}

// ─── ImportOutcome ───

/// 单个文件的导入结果。
#[derive(Debug, Clone)]
pub enum ImportOutcome {
    /// 成功导入。
    Imported(ImportedArtifact),
    /// 去重命中（SHA256 + size 匹配已有记录）。
    Deduplicated {
        asset_id: String,
        source_path: String,
        reason: String,
    },
    /// 导入失败。
    Failed { source_path: String, reason: String },
}

// ─── ArtifactImporter ───

/// Creative Runtime 资产导入器。
///
/// 薄包装层：验证输入 → 调用 AssetService.import() → 映射为 ImportedArtifact。
/// SHA256 计算、staging、去重、持久化全部由 AssetService 内部完成。
pub struct ArtifactImporter {
    asset_service: Arc<AssetService>,
}

impl ArtifactImporter {
    pub fn new(asset_service: Arc<AssetService>) -> Self {
        Self { asset_service }
    }

    /// 导入单个资产文件。
    pub fn import_one(&self, input: ImportInput) -> Result<ImportedArtifact, String> {
        // 1. 验证文件存在
        let path = std::path::Path::new(&input.file_path);
        if !path.exists() {
            return Err(format!("[Importer] file not found: {}", input.file_path));
        }
        if !path.is_file() {
            return Err(format!(
                "[Importer] not a regular file: {}",
                input.file_path
            ));
        }

        // 2. 调用 AssetService.import()
        let summary = self
            .asset_service
            .import(vec![input.file_path.clone()], input.namespace.clone())
            .map_err(|e| format!("[Importer] AssetService.import failed: {e}"))?;

        // 3. 映射结果
        if let Some(outcome) = summary.imported.into_iter().next() {
            let asset = outcome.asset;
            let metadata = build_creative_metadata(&input, &asset);

            eprintln!(
                "[Importer] imported: {} → {} (sha256={})",
                input.file_path,
                asset.relative_path,
                asset.sha256.as_deref().unwrap_or("none")
            );

            Ok(ImportedArtifact {
                asset_id: asset.id,
                relative_path: asset.relative_path,
                sha256: asset.sha256,
                size_bytes: asset.size_bytes,
                mime_type: asset.mime_type,
                revision: asset.revision,
                metadata,
            })
        } else if let Some(skipped) = summary.skipped.into_iter().next() {
            // 去重命中：仍视为成功（返回已有记录的信息）
            if let Some(existing_id) = &skipped.existing_asset_id {
                eprintln!(
                    "[Importer] deduplicated: {} → existing {}",
                    input.file_path, existing_id
                );
                // 尝试通过 AssetService.get() 获取已有记录
                match self.asset_service.get(existing_id) {
                    Ok(Some(asset)) => {
                        let metadata = build_creative_metadata(&input, &asset);
                        Ok(ImportedArtifact {
                            asset_id: asset.id,
                            relative_path: asset.relative_path,
                            sha256: asset.sha256,
                            size_bytes: asset.size_bytes,
                            mime_type: asset.mime_type,
                            revision: asset.revision,
                            metadata,
                        })
                    }
                    _ => Err(format!(
                        "[Importer] dedup hit but cannot fetch record: {}",
                        existing_id
                    )),
                }
            } else {
                Err(format!(
                    "[Importer] skipped without existing_id: {}",
                    skipped.reason
                ))
            }
        } else if let Some(failure) = summary.failures.into_iter().next() {
            Err(format!(
                "[Importer] import failed for {}: {}",
                failure.source_path, failure.reason
            ))
        } else {
            Err("[Importer] empty summary from AssetService".to_owned())
        }
    }

    /// 批量导入（单文件失败不影响其他）。
    pub fn import_batch(&self, inputs: Vec<ImportInput>) -> Vec<ImportOutcome> {
        inputs
            .into_iter()
            .map(|input| {
                let file_path = input.file_path.clone();
                match self.import_one(input) {
                    Ok(artifact) => ImportOutcome::Imported(artifact),
                    Err(reason) => {
                        // 检查是否为去重命中（reason 包含 "deduplicated"）
                        if reason.contains("dedup") {
                            ImportOutcome::Deduplicated {
                                asset_id: String::new(),
                                source_path: file_path,
                                reason,
                            }
                        } else {
                            ImportOutcome::Failed {
                                source_path: file_path,
                                reason,
                            }
                        }
                    }
                }
            })
            .collect()
    }
}

// ─── Helpers ───

/// 构建创作上下文 metadata（附加到 ImportedArtifact.metadata）。
fn build_creative_metadata(
    input: &ImportInput,
    _asset: &crate::domain::assets::AssetRecord,
) -> HashMap<String, serde_json::Value> {
    let mut metadata = HashMap::new();

    metadata.insert(
        "asset_type".to_owned(),
        serde_json::to_value(&input.asset_type).unwrap_or_default(),
    );
    metadata.insert(
        "provider_id".to_owned(),
        serde_json::Value::String(input.provider_id.clone()),
    );
    metadata.insert(
        "model".to_owned(),
        serde_json::Value::String(input.model.clone()),
    );
    metadata.insert(
        "step_index".to_owned(),
        serde_json::Value::Number(input.step_index.into()),
    );
    metadata.insert(
        "shot_index".to_owned(),
        serde_json::Value::Number(input.shot_index.into()),
    );
    if let Some(sub_id) = &input.submission_id {
        metadata.insert(
            "submission_id".to_owned(),
            serde_json::Value::String(sub_id.clone()),
        );
    }
    if let Some(src_id) = &input.source_artifact_id {
        metadata.insert(
            "source_artifact_id".to_owned(),
            serde_json::Value::String(src_id.clone()),
        );
    }

    metadata
}

// ─── Tests ───

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_import_input_construction() {
        let input = ImportInput {
            file_path: "/tmp/downloaded/image_001.png".to_owned(),
            display_name: "镜头1-城市夜景".to_owned(),
            namespace: "generation".to_owned(),
            asset_type: AssetType::Image,
            submission_id: Some("sub-abc123".to_owned()),
            provider_id: "grok".to_owned(),
            model: "gpt-image-1".to_owned(),
            step_index: 2,
            shot_index: 1,
            source_artifact_id: Some("img-002-abc".to_owned()),
        };

        assert_eq!(input.file_path, "/tmp/downloaded/image_001.png");
        assert_eq!(input.asset_type, AssetType::Image);
        assert_eq!(input.submission_id, Some("sub-abc123".to_owned()));
        assert_eq!(input.source_artifact_id, Some("img-002-abc".to_owned()));
    }

    #[test]
    fn test_creative_metadata() {
        let input = ImportInput {
            file_path: "/tmp/test.mp4".to_owned(),
            display_name: "test".to_owned(),
            namespace: "generation".to_owned(),
            asset_type: AssetType::Video,
            submission_id: Some("sub-xyz".to_owned()),
            provider_id: "kling".to_owned(),
            model: "kling-v2".to_owned(),
            step_index: 3,
            shot_index: 2,
            source_artifact_id: Some("vid-003-xyz".to_owned()),
        };

        // 构建一个最小 AssetRecord 用于 metadata 构建
        let asset = crate::domain::assets::AssetRecord {
            id: "asset-001".to_owned(),
            storage_namespace: crate::domain::assets::StorageNamespace::Generation,
            asset_kind: crate::domain::assets::AssetKind::Video,
            display_name: "test".to_owned(),
            relative_path: "assets/test.mp4".to_owned(),
            size_bytes: 1024,
            sha256: Some("abc123def456".to_owned()),
            mime_type: Some("video/mp4".to_owned()),
            integrity_status: crate::domain::assets::IntegrityStatus::Valid,
            metadata_json: "{}".to_owned(),
            origin_device_id: None,
            revision: 1,
            created_at: "2026-07-23T00:00:00Z".to_owned(),
            updated_at: "2026-07-23T00:00:00Z".to_owned(),
        };

        let metadata = build_creative_metadata(&input, &asset);

        assert_eq!(
            metadata.get("provider_id"),
            Some(&serde_json::Value::String("kling".to_owned()))
        );
        assert_eq!(
            metadata.get("model"),
            Some(&serde_json::Value::String("kling-v2".to_owned()))
        );
        assert_eq!(
            metadata.get("step_index"),
            Some(&serde_json::Value::Number(3.into()))
        );
        assert_eq!(
            metadata.get("shot_index"),
            Some(&serde_json::Value::Number(2.into()))
        );
        assert_eq!(
            metadata.get("submission_id"),
            Some(&serde_json::Value::String("sub-xyz".to_owned()))
        );
        assert_eq!(
            metadata.get("source_artifact_id"),
            Some(&serde_json::Value::String("vid-003-xyz".to_owned()))
        );
    }

    #[test]
    fn test_import_outcome_variants() {
        let imported = ImportOutcome::Imported(ImportedArtifact {
            asset_id: "asset-001".to_owned(),
            relative_path: "assets/test.png".to_owned(),
            sha256: Some("abc".to_owned()),
            size_bytes: 512,
            mime_type: Some("image/png".to_owned()),
            revision: 1,
            metadata: HashMap::new(),
        });
        assert!(matches!(imported, ImportOutcome::Imported(_)));

        let dedup = ImportOutcome::Deduplicated {
            asset_id: "asset-existing".to_owned(),
            source_path: "/tmp/dup.png".to_owned(),
            reason: "SHA256 match".to_owned(),
        };
        assert!(matches!(dedup, ImportOutcome::Deduplicated { .. }));

        let failed = ImportOutcome::Failed {
            source_path: "/tmp/bad.png".to_owned(),
            reason: "file not found".to_owned(),
        };
        assert!(matches!(failed, ImportOutcome::Failed { .. }));
    }
}
