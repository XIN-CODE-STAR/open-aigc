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

use std::path::{Path, PathBuf};
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

/// 检测系统中可用的 ffmpeg。
///
/// 探测顺序（技术决策 D2）：应用可执行文件同目录的 sidecar
/// （tauri externalBin 随应用分发的 `ffmpeg-<target-triple>.exe`）→ PATH。
/// sidecar 候选只做"存在 + 体积"门禁而不额外起进程探测：
/// 部署时 fetch-ffmpeg.mjs 已做过 `-version`/libx264 冒烟，
/// 运行期的坏文件会在首次 compose 时以明确错误暴露。
/// 返回 None 时由调用方降级（mock 合成 / 不注册 Composite skill）。
pub(crate) fn detect_ffmpeg() -> Option<String> {
    if let Some(path) = detect_ffmpeg_sidecar() {
        return Some(path);
    }
    std::process::Command::new("ffmpeg")
        .arg("-version")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .ok()
        .filter(|s| s.success())
        .map(|_| "ffmpeg".to_owned())
}

/// 在可执行文件同目录探测 ffmpeg sidecar。
fn detect_ffmpeg_sidecar() -> Option<String> {
    let exe = std::env::current_exe().ok()?;
    let dir = exe.parent()?;
    ffmpeg_sidecar_candidates(dir)
        .into_iter()
        .find(|p| looks_like_ffmpeg_binary(p))
        .map(|p| p.to_string_lossy().to_string())
}

/// sidecar 候选路径（按优先级）：externalBin 约定命名 → 手工放置的裸名。
/// 纯函数，便于单测。
fn ffmpeg_sidecar_candidates(dir: &Path) -> Vec<PathBuf> {
    let suffix = std::env::consts::EXE_SUFFIX;
    vec![
        dir.join(format!("ffmpeg-x86_64-pc-windows-msvc{suffix}")),
        dir.join(format!("ffmpeg{suffix}")),
    ]
}

/// 静态构建的 ffmpeg 二进制约 100MB；1MB 门禁足以排除占位文件与截断残留。
/// （共享构建的数百 KB exe + DLL 是另一种分发形态，不进 sidecar 约定。）
const FFMPEG_MIN_BYTES: u64 = 1024 * 1024;

/// 判断候选路径是否像一份 ffmpeg 部署产物（供探测链与测试复用）。
pub(crate) fn looks_like_ffmpeg_binary(path: &Path) -> bool {
    match std::fs::metadata(path) {
        Ok(meta) => meta.is_file() && meta.len() >= FFMPEG_MIN_BYTES,
        Err(_) => false,
    }
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
        // 环境依赖测试：sidecar 或 PATH 任一命中即可，两者皆无时返回 None。
        // 只确保不 panic 且非空，不锁定具体命中来源。
        if let Some(path) = super::detect_ffmpeg() {
            assert!(!path.is_empty());
        }
    }

    #[test]
    fn test_ffmpeg_sidecar_candidates_order() {
        let dir = PathBuf::from("/app");
        let candidates = super::ffmpeg_sidecar_candidates(&dir);
        assert_eq!(candidates.len(), 2);

        let suffix = std::env::consts::EXE_SUFFIX;
        // 首选 externalBin 约定命名（tauri 会把 sidecar 复制到可执行文件旁）
        assert_eq!(
            candidates[0].file_name().unwrap().to_string_lossy(),
            format!("ffmpeg-x86_64-pc-windows-msvc{suffix}")
        );
        // 次选裸名，作为手工放置的兼容入口
        assert_eq!(
            candidates[1].file_name().unwrap().to_string_lossy(),
            format!("ffmpeg{suffix}")
        );
    }

    #[test]
    fn test_looks_like_ffmpeg_binary_rejects_placeholder() {
        let path = std::env::temp_dir().join(format!("aigc_ffmpeg_probe_{}", uuid::Uuid::new_v4()));
        std::fs::write(&path, b"placeholder").unwrap();
        assert!(!super::looks_like_ffmpeg_binary(&path));
        let _ = std::fs::remove_file(&path);
    }
}
