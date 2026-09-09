#![allow(dead_code)]
//! Composite Skill：多素材合成 Skill。
//!
//! 职责链：收集上游视觉 Artifact → 构造 CompositionRequest → Facade.compose() → 输出 Artifact。
//!
//! CompositeSkill 不直接调用 ffmpeg，而是通过 CompositionFacade 抽象层。
//! 与 ImageGenerationSkill / VideoGenerationSkill 的区别：
//! - 不走 GenerationFacade（不是 AI 生成，是本地工具调用）
//! - 需要读取 runtime.artifact_store() 获取上游素材
//!
//! Future: async worker for long-running composition

use std::sync::Arc;

use crate::{
    application::{
        composition_facade::{CompositionFacade, CompositionRequest, MediaInput},
        execution_runtime::ExecutionRuntime,
        sequential_executor::ExecutionSkill,
    },
    domain::{
        agent::PlanStepKind,
        creative_plan::AssetType,
        execution::{Artifact, ArtifactOrigin, Provenance},
    },
};

// ─── CompositeSkill ───

/// 多素材合成 Skill：通过 CompositionFacade 将多个视觉素材合成为单个视频。
pub struct CompositeSkill {
    facade: Arc<dyn CompositionFacade>,
    /// 工作区根目录（运行时拼接 workspace_id 得到完整输出路径）。
    workspace_root: String,
}

impl CompositeSkill {
    pub fn new(facade: Arc<dyn CompositionFacade>, workspace_root: String) -> Self {
        Self {
            facade,
            workspace_root,
        }
    }

    /// 创建使用 Mock Facade 的实例（测试用）。
    #[cfg(test)]
    pub fn with_mock(workspace_root: String) -> Self {
        use crate::application::composition_facade::MockCompositionFacade;
        Self {
            facade: Arc::new(MockCompositionFacade),
            workspace_root,
        }
    }
}

impl ExecutionSkill for CompositeSkill {
    fn name(&self) -> &str {
        "Composite"
    }

    fn capability(&self) -> PlanStepKind {
        PlanStepKind::Composite
    }

    fn execute(
        &self,
        step_index: u8,
        _step_description: &str,
        runtime: &ExecutionRuntime,
    ) -> Result<Artifact, String> {
        // 1. 从 ArtifactStore 收集上游视觉素材
        let store = runtime.artifact_store();
        let visual_artifacts: Vec<&Artifact> = store
            .all()
            .iter()
            .filter(|a| {
                matches!(a.asset_type, AssetType::Image | AssetType::Video)
                    && !matches!(a.origin, ArtifactOrigin::Mock)
            })
            .collect();

        if visual_artifacts.is_empty() {
            return Err("no visual artifacts to composite".to_owned());
        }

        // 2. 构造 MediaInput 列表（按 shot_index 排序）
        let input_count = visual_artifacts.len();
        let mut sorted = visual_artifacts;
        sorted.sort_by_key(|a| a.shot_index);

        let inputs: Vec<MediaInput> = sorted
            .iter()
            .map(|a| MediaInput {
                file_path: a.location.clone(),
                asset_type: a.asset_type.clone(),
                duration_secs: match a.asset_type {
                    AssetType::Image => 3.0,
                    AssetType::Video => 5.0,
                    _ => 3.0,
                },
            })
            .collect();

        // 3. 构造输出路径（workspace_root / workspace_id / compose/）
        let output_dir = format!("{}/{}/compose", self.workspace_root, runtime.workspace_id());

        let request = CompositionRequest {
            inputs,
            output_dir,
            output_filename: format!(
                "composed-step{step_index:03}-{}.mp4",
                uuid::Uuid::new_v4()
                    .to_string()
                    .split('-')
                    .next()
                    .unwrap_or("0")
            ),
        };

        // 4. 调用 Facade
        let output = self.facade.compose(&request)?;

        // 5. 构造 Artifact
        let mut metadata = output.metadata;
        metadata.insert(
            "input_count".to_owned(),
            serde_json::Value::Number(input_count.into()),
        );
        metadata.insert(
            "duration_secs".to_owned(),
            serde_json::Value::Number(
                serde_json::Number::from_f64(output.duration_secs).unwrap_or(0.into()),
            ),
        );

        Ok(Artifact {
            id: format!(
                "comp-{step_index:03}-{}",
                uuid::Uuid::new_v4()
                    .to_string()
                    .split('-')
                    .next()
                    .unwrap_or("0")
            ),
            source_step: step_index,
            shot_index: 0, // 合成输出不对应单个镜头
            asset_type: AssetType::Video,
            location: output.file_path,
            origin: ArtifactOrigin::Composed,
            metadata,
            provenance: Some(Provenance::skill_only("CompositeSkill")),
        })
    }
}

// ─── FFmpeg Detection ───

/// 检测系统中是否安装了 ffmpeg。
///
/// 尝试执行 `ffmpeg -version`，成功则返回 ffmpeg 路径。
pub(crate) fn detect_ffmpeg() -> Option<String> {
    std::process::Command::new("ffmpeg")
        .arg("-version")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .ok()
        .filter(|s| s.success())
        .map(|_| "ffmpeg".to_owned())
}

// ─── Tests ───

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::execution::ArtifactOrigin;

    fn make_runtime_with_artifacts(artifacts: Vec<Artifact>) -> ExecutionRuntime {
        let mut runtime = ExecutionRuntime::new("plan-test".to_owned(), "ws-test".to_owned());
        for (i, artifact) in artifacts.into_iter().enumerate() {
            runtime.complete_step(i as u8, artifact);
        }
        runtime
    }

    fn sample_video_artifact(shot_index: u8) -> Artifact {
        Artifact {
            id: format!("vid-{shot_index:03}-abc"),
            source_step: shot_index,
            shot_index,
            asset_type: AssetType::Video,
            location: format!("/workspace/shot_{shot_index}.mp4"),
            origin: ArtifactOrigin::Generated,
            metadata: std::collections::HashMap::new(),
            provenance: None,
        }
    }

    fn sample_image_artifact(shot_index: u8) -> Artifact {
        Artifact {
            id: format!("img-{shot_index:03}-def"),
            source_step: shot_index,
            shot_index,
            asset_type: AssetType::Image,
            location: format!("/workspace/shot_{shot_index}.png"),
            origin: ArtifactOrigin::Generated,
            metadata: std::collections::HashMap::new(),
            provenance: None,
        }
    }

    #[test]
    fn test_composite_skill_no_artifacts() {
        let skill = CompositeSkill::with_mock("/tmp/workspace".to_owned());
        let runtime = ExecutionRuntime::new("plan-001".to_owned(), "ws-001".to_owned());

        let result = skill.execute(1, "合成", &runtime);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("no visual artifacts"));
    }

    #[test]
    fn test_composite_skill_with_mock_facade() {
        let skill = CompositeSkill::with_mock("/tmp/workspace".to_owned());

        let artifacts = vec![
            sample_video_artifact(1),
            sample_image_artifact(2),
            sample_video_artifact(3),
        ];
        let runtime = make_runtime_with_artifacts(artifacts);

        let result = skill.execute(4, "合成3个素材", &runtime);
        assert!(result.is_ok());

        let artifact = result.unwrap();
        assert_eq!(artifact.origin, ArtifactOrigin::Composed);
        assert_eq!(artifact.asset_type, AssetType::Video);
        assert_eq!(artifact.shot_index, 0);
        assert!(artifact.id.starts_with("comp-"));
        assert!(artifact.location.contains("composed-step004"));
        // 验证 metadata 包含 input_count
        let input_count = artifact
            .metadata
            .get("input_count")
            .unwrap()
            .as_u64()
            .unwrap();
        assert_eq!(input_count, 3);
    }

    #[test]
    fn test_composite_skill_skips_mock_artifacts() {
        let skill = CompositeSkill::with_mock("/tmp/workspace".to_owned());

        // 只有 Mock artifact，应该被过滤掉
        let mock_artifact = Artifact {
            id: "mock-vid-001".to_owned(),
            source_step: 1,
            shot_index: 1,
            asset_type: AssetType::Video,
            location: "artifact://mock/video_001.mp4".to_owned(),
            origin: ArtifactOrigin::Mock,
            metadata: std::collections::HashMap::new(),
            provenance: None,
        };
        let runtime = make_runtime_with_artifacts(vec![mock_artifact]);

        let result = skill.execute(2, "合成", &runtime);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("no visual artifacts"));
    }

    #[test]
    fn test_composite_skill_mixed_media_sorted_by_shot() {
        let skill = CompositeSkill::with_mock("/tmp/workspace".to_owned());

        // 乱序投入：shot 3 → shot 1 → shot 2
        let artifacts = vec![
            sample_video_artifact(3),
            sample_image_artifact(1),
            sample_video_artifact(2),
        ];
        let runtime = make_runtime_with_artifacts(artifacts);

        let result = skill.execute(4, "合成", &runtime);
        assert!(result.is_ok());

        let artifact = result.unwrap();
        let input_count = artifact
            .metadata
            .get("input_count")
            .unwrap()
            .as_u64()
            .unwrap();
        assert_eq!(input_count, 3);
        // duration = 5.0 + 3.0 + 5.0 = 13.0（video=5s, image=3s, video=5s）
        let duration = artifact
            .metadata
            .get("duration_secs")
            .unwrap()
            .as_f64()
            .unwrap();
        assert!((duration - 13.0).abs() < 0.01);
    }

    #[test]
    fn test_detect_ffmpeg() {
        // 环境依赖测试：可能通过也可能失败
        let result = super::detect_ffmpeg();
        // 不做断言，只确保不 panic
        if let Some(path) = result {
            assert_eq!(path, "ffmpeg");
        }
    }
}
