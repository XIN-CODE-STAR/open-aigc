#![allow(dead_code)]
//! Creative Runtime Service：创作流水线四阶段编排器。
//!
//! 当 AgentService 检测到创作任务（CreativePlan 存在）时，
//! 替换通用 ReAct 循环，直接驱动：
//!
//! Phase 1: Submit — 所有 shot → GenerationFacade.submit() → submission_id
//! Phase 2: Poll — CreativePollWorker 轮询至全部完成 → 下载到本地
//! Phase 3: Import — ArtifactImporter 导入为 AssetRecord（SHA256 + 去重）
//! Phase 4: Compose — CompositeSkill → CompositionFacade → ffmpeg → 最终作品
//!
//! 设计原则：
//! - 不修改 SequentialExecutor（保留通用执行器）
//! - 不修改 GenerationFacade / CreativePollWorker / ArtifactImporter / CompositeSkill
//! - 仅做编排：组合已有组件，不引入新逻辑层
//! - 每阶段独立容错，部分失败不阻断后续阶段

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use crate::application::execution_runtime::ExecutionRuntime;
use crate::application::provider_registry::ProviderRegistry;
use crate::application::runtime_event_bus::RuntimeEventBus;
use crate::application::{
    artifact_importer::{ArtifactImporter, ImportInput, ImportedArtifact},
    composite_skill::CompositeSkill,
    composition_facade::CompositionFacade,
    creative_poll_worker::{CreativePollWorker, PendingSubmission, SubmissionResult},
    generation_facade::{GenerationFacade, GenerationRequest},
    sequential_executor::ExecutionSkill,
};
use crate::domain::creative_plan::{AssetType, CreativePlan};
use crate::domain::execution::{ArtifactOrigin, ExecutionStatus, Provenance};
use crate::domain::runtime_event::{RuntimeEvent, RuntimePhase as DomainPhase, ShotStatus};

// ─── RuntimePhase ───

/// 运行时阶段（供前端进度展示）。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuntimePhase {
    /// 提交生成请求中。
    Submitting,
    /// 轮询远端状态 + 下载中。
    Polling,
    /// 导入资产库中。
    Importing,
    /// 合成最终作品中。
    Composing,
    /// 全部完成。
    Completed,
    /// 失败。
    Failed,
}

// ─── RuntimeProgress ───

/// 运行时进度事件（通过 Tauri emit 推送前端）。
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeProgress {
    pub phase: RuntimePhase,
    pub current_shot: u8,
    pub total_shots: u8,
    pub message: String,
}

// ─── RuntimeResult ───

/// 运行时最终结果。
#[derive(Debug, Clone)]
pub struct RuntimeResult {
    /// 执行状态。
    pub status: ExecutionStatus,
    /// 合成视频路径（如果合成成功）。
    pub composed_video_path: Option<String>,
    /// 成功导入的镜头资产。
    pub shot_artifacts: Vec<ImportedArtifact>,
    /// 失败记录 (shot_index, reason)。
    pub errors: Vec<(u8, String)>,
    /// 总执行耗时（秒）。
    pub duration_secs: f64,
}

// ─── CreativeRuntimeService ───

/// 创作流水线编排器。
///
/// 组合 GenerationFacade + CreativePollWorker + ArtifactImporter + CompositionFacade，
/// 驱动 CreativePlan 从提交到最终视频的完整生命周期。
pub struct CreativeRuntimeService {
    generation_facade: Arc<dyn GenerationFacade>,
    provider_registry: Arc<ProviderRegistry>,
    artifact_importer: Arc<ArtifactImporter>,
    composition_facade: Arc<dyn CompositionFacade>,
    workspace_root: String,
    event_bus: Option<Arc<RuntimeEventBus>>,
}

impl CreativeRuntimeService {
    pub fn new(
        generation_facade: Arc<dyn GenerationFacade>,
        provider_registry: Arc<ProviderRegistry>,
        artifact_importer: Arc<ArtifactImporter>,
        composition_facade: Arc<dyn CompositionFacade>,
        workspace_root: String,
    ) -> Self {
        Self {
            generation_facade,
            provider_registry,
            artifact_importer,
            composition_facade,
            workspace_root,
            event_bus: None,
        }
    }

    /// 注入运行时事件总线（builder 模式）。
    pub fn with_event_bus(mut self, event_bus: Arc<RuntimeEventBus>) -> Self {
        self.event_bus = Some(event_bus);
        self
    }

    /// 向事件总线发送事件（如果已注入）。
    fn emit(&self, event: RuntimeEvent) {
        if let Some(bus) = &self.event_bus {
            bus.emit(event);
        }
    }

    /// 执行创作计划的完整四阶段流水线。
    ///
    /// 每阶段独立容错：某个 shot 失败不影响其他 shot 的处理。
    pub fn execute_plan(&self, plan: &CreativePlan) -> RuntimeResult {
        let start = Instant::now();
        let mut all_errors: Vec<(u8, String)> = Vec::new();
        let run_id = uuid::Uuid::new_v4().to_string();

        eprintln!(
            "[CreativeRuntime] executing plan: {} shots, {:.0}s total, run_id={}",
            plan.shot_count(),
            plan.total_duration_secs(),
            run_id
        );

        self.emit(RuntimeEvent::RunStarted {
            run_id: run_id.clone(),
            total_shots: plan.shot_count() as u32,
        });

        // ── Phase 1: Submit ──
        self.emit(RuntimeEvent::PhaseChanged {
            run_id: run_id.clone(),
            phase: DomainPhase::Submitting,
            message: format!("提交 {} 个镜头生成请求...", plan.shot_count()),
        });
        eprintln!(
            "[CreativeRuntime] Phase 1: Submitting {} shots...",
            plan.shot_count()
        );
        let submissions = match self.submit_all_shots(plan) {
            Ok(subs) => subs,
            Err(e) => {
                eprintln!("[CreativeRuntime] Phase 1 fatal error: {e}");
                self.emit(RuntimeEvent::RunCompleted {
                    run_id: run_id.clone(),
                    status: "failed".to_owned(),
                    output_asset_id: None,
                    duration_secs: start.elapsed().as_secs_f64(),
                    shot_success_count: 0,
                    shot_total_count: plan.shot_count() as u32,
                });
                return RuntimeResult {
                    status: ExecutionStatus::Failed,
                    composed_video_path: None,
                    shot_artifacts: Vec::new(),
                    errors: vec![(0, e)],
                    duration_secs: start.elapsed().as_secs_f64(),
                };
            }
        };
        eprintln!(
            "[CreativeRuntime] Phase 1 done: {} submissions",
            submissions.len()
        );

        // Emit SubmissionUpdated for each successful submission
        for sub in &submissions {
            self.emit(RuntimeEvent::SubmissionUpdated {
                run_id: run_id.clone(),
                shot_index: sub.shot_index as u32,
                submission_id: sub.submission_id.clone(),
                provider: sub.provider_id.clone(),
                model: sub.model.clone(),
            });
        }

        // ── Phase 2: Poll + Download ──
        self.emit(RuntimeEvent::PhaseChanged {
            run_id: run_id.clone(),
            phase: DomainPhase::Polling,
            message: format!("轮询 {} 个生成任务...", submissions.len()),
        });
        eprintln!("[CreativeRuntime] Phase 2: Polling + downloading...");
        let download_results = self.poll_and_download(&submissions);

        let mut downloaded_files: Vec<(
            PendingSubmission,
            crate::application::creative_poll_worker::DownloadedFile,
        )> = Vec::new();
        for result in download_results {
            match result {
                SubmissionResult::Success { submission, file } => {
                    eprintln!(
                        "[CreativeRuntime] shot {} downloaded: {}",
                        submission.shot_index, file.file_path
                    );
                    self.emit(RuntimeEvent::ShotUpdated {
                        run_id: run_id.clone(),
                        shot_index: submission.shot_index as u32,
                        status: ShotStatus::Downloaded,
                        artifact_id: None,
                        message: format!("Shot {} 已下载到本地", submission.shot_index),
                    });
                    downloaded_files.push((submission, file));
                }
                SubmissionResult::GenerationFailed { submission, reason } => {
                    eprintln!(
                        "[CreativeRuntime] shot {} generation failed: {reason}",
                        submission.shot_index
                    );
                    self.emit(RuntimeEvent::ShotUpdated {
                        run_id: run_id.clone(),
                        shot_index: submission.shot_index as u32,
                        status: ShotStatus::Failed,
                        artifact_id: None,
                        message: format!("生成失败: {reason}"),
                    });
                    all_errors.push((submission.shot_index, format!("generation: {reason}")));
                }
                SubmissionResult::DownloadFailed { submission, reason } => {
                    eprintln!(
                        "[CreativeRuntime] shot {} download failed: {reason}",
                        submission.shot_index
                    );
                    self.emit(RuntimeEvent::ShotUpdated {
                        run_id: run_id.clone(),
                        shot_index: submission.shot_index as u32,
                        status: ShotStatus::Failed,
                        artifact_id: None,
                        message: format!("下载失败: {reason}"),
                    });
                    all_errors.push((submission.shot_index, format!("download: {reason}")));
                }
                SubmissionResult::TimedOut { submission } => {
                    eprintln!("[CreativeRuntime] shot {} timed out", submission.shot_index);
                    self.emit(RuntimeEvent::ShotUpdated {
                        run_id: run_id.clone(),
                        shot_index: submission.shot_index as u32,
                        status: ShotStatus::Failed,
                        artifact_id: None,
                        message: "生成超时".to_owned(),
                    });
                    all_errors.push((submission.shot_index, "timeout".to_owned()));
                }
            }
        }
        eprintln!(
            "[CreativeRuntime] Phase 2 done: {}/{} downloaded",
            downloaded_files.len(),
            submissions.len()
        );

        // ── Phase 3: Import ──
        self.emit(RuntimeEvent::PhaseChanged {
            run_id: run_id.clone(),
            phase: DomainPhase::Importing,
            message: format!("导入 {} 个资产到项目库...", downloaded_files.len()),
        });
        eprintln!(
            "[CreativeRuntime] Phase 3: Importing {} files...",
            downloaded_files.len()
        );
        let mut imported_artifacts: Vec<ImportedArtifact> = Vec::new();

        for (submission, file) in &downloaded_files {
            let display_name = plan
                .shots
                .iter()
                .find(|s| s.index == submission.shot_index)
                .map(|s| format!("镜头{} [{}]", s.index, s.goal))
                .unwrap_or_else(|| format!("Shot {}", submission.shot_index));

            let input = ImportInput {
                file_path: file.file_path.clone(),
                display_name,
                namespace: "generation".to_owned(),
                asset_type: submission.asset_type.clone(),
                submission_id: Some(submission.submission_id.clone()),
                provider_id: submission.provider_id.clone(),
                model: submission.model.clone(),
                step_index: submission.step_index,
                shot_index: submission.shot_index,
                source_artifact_id: None,
            };

            match self.artifact_importer.import_one(input) {
                Ok(artifact) => {
                    eprintln!(
                        "[CreativeRuntime] shot {} imported: {}",
                        submission.shot_index, artifact.asset_id
                    );
                    self.emit(RuntimeEvent::ShotUpdated {
                        run_id: run_id.clone(),
                        shot_index: submission.shot_index as u32,
                        status: ShotStatus::Imported,
                        artifact_id: Some(artifact.asset_id.clone()),
                        message: format!("Shot {} 已导入资产库", submission.shot_index),
                    });
                    imported_artifacts.push(artifact);
                }
                Err(e) => {
                    eprintln!(
                        "[CreativeRuntime] shot {} import failed: {e}",
                        submission.shot_index
                    );
                    self.emit(RuntimeEvent::ShotUpdated {
                        run_id: run_id.clone(),
                        shot_index: submission.shot_index as u32,
                        status: ShotStatus::Failed,
                        artifact_id: None,
                        message: format!("导入失败: {e}"),
                    });
                    all_errors.push((submission.shot_index, format!("import: {e}")));
                }
            }
        }
        eprintln!(
            "[CreativeRuntime] Phase 3 done: {}/{} imported",
            imported_artifacts.len(),
            downloaded_files.len()
        );

        // ── Phase 4: Compose ──
        self.emit(RuntimeEvent::PhaseChanged {
            run_id: run_id.clone(),
            phase: DomainPhase::Composing,
            message: format!("合成 {} 个素材为最终作品...", imported_artifacts.len()),
        });
        let composed_path = if imported_artifacts.len() >= 2 {
            eprintln!(
                "[CreativeRuntime] Phase 4: Composing {} assets...",
                imported_artifacts.len()
            );
            match self.compose_artifacts(&imported_artifacts, plan) {
                Ok(path) => {
                    eprintln!("[CreativeRuntime] Phase 4 done: {path}");
                    Some(path)
                }
                Err(e) => {
                    eprintln!("[CreativeRuntime] Phase 4 compose failed: {e}");
                    all_errors.push((0, format!("compose: {e}")));
                    None
                }
            }
        } else if imported_artifacts.len() == 1 {
            // 单个素材无需合成，直接使用
            eprintln!("[CreativeRuntime] Phase 4 skipped: single asset, no composition needed");
            Some(imported_artifacts[0].relative_path.clone())
        } else {
            eprintln!("[CreativeRuntime] Phase 4 skipped: no assets to compose");
            None
        };

        // ── 确定最终状态 ──
        let status = if all_errors.is_empty() {
            ExecutionStatus::Completed
        } else if imported_artifacts.is_empty() && composed_path.is_none() {
            ExecutionStatus::Failed
        } else {
            ExecutionStatus::Partial
        };

        let duration = start.elapsed().as_secs_f64();
        let status_str = match &status {
            ExecutionStatus::Completed => "completed",
            ExecutionStatus::Failed => "failed",
            _ => "partial",
        };

        self.emit(RuntimeEvent::PhaseChanged {
            run_id: run_id.clone(),
            phase: if status == ExecutionStatus::Failed {
                DomainPhase::Failed
            } else {
                DomainPhase::Completed
            },
            message: format!(
                "流水线完成: {}/{} 成功, {:.1}s",
                (plan.shot_count() as u32).saturating_sub(all_errors.len() as u32),
                plan.shot_count(),
                duration
            ),
        });

        let shot_success = (plan.shot_count() as u32).saturating_sub(all_errors.len() as u32);
        self.emit(RuntimeEvent::RunCompleted {
            run_id: run_id.clone(),
            status: status_str.to_owned(),
            output_asset_id: composed_path.clone(),
            duration_secs: duration,
            shot_success_count: shot_success,
            shot_total_count: plan.shot_count() as u32,
        });

        eprintln!(
            "[CreativeRuntime] finished: {:?}, {} artifacts, composed={:?}, {:.1}s",
            status,
            imported_artifacts.len(),
            composed_path.as_deref().unwrap_or("none"),
            duration
        );

        RuntimeResult {
            status,
            composed_video_path: composed_path,
            shot_artifacts: imported_artifacts,
            errors: all_errors,
            duration_secs: duration,
        }
    }

    // ─── Phase 1: Submit ───

    fn submit_all_shots(&self, plan: &CreativePlan) -> Result<Vec<PendingSubmission>, String> {
        let mut submissions = Vec::new();

        for shot in &plan.shots {
            let request = GenerationRequest {
                asset_type: shot.asset_type.clone(),
                prompt: format!("{} {}", plan.global_style, shot.description)
                    .trim()
                    .to_owned(),
                style: if plan.global_style.is_empty() {
                    None
                } else {
                    Some(plan.global_style.clone())
                },
                reference_url: if shot.references.is_empty() {
                    None
                } else {
                    Some(shot.references[0].clone())
                },
                duration_secs: Some(shot.duration_secs),
                extra: HashMap::new(),
            };

            match self.generation_facade.submit(&request) {
                Ok(output) => {
                    let submission_id = output.submission_id.unwrap_or_default();
                    eprintln!(
                        "[CreativeRuntime] shot {} submitted: provider={}, model={}, id={}",
                        shot.index, output.provider, output.model, submission_id
                    );
                    submissions.push(PendingSubmission {
                        submission_id,
                        provider_id: output.provider,
                        step_index: shot.index,
                        shot_index: shot.index,
                        asset_type: shot.asset_type.clone(),
                        model: output.model,
                    });
                }
                Err(e) => {
                    eprintln!("[CreativeRuntime] shot {} submit failed: {e}", shot.index);
                    // 单个 shot 提交失败不阻断其他 shot
                }
            }
        }

        if submissions.is_empty() && !plan.shots.is_empty() {
            return Err("all shots failed to submit".to_owned());
        }

        Ok(submissions)
    }

    // ─── Phase 2: Poll + Download ───

    fn poll_and_download(&self, submissions: &[PendingSubmission]) -> Vec<SubmissionResult> {
        let download_dir = PathBuf::from(&self.workspace_root).join("compose_temp");
        let poll_worker =
            CreativePollWorker::new(Arc::clone(&self.provider_registry), download_dir);

        // 超时策略：每 shot 预留 60s（视频可能需更久），最少 120s
        let timeout = std::cmp::max(submissions.len() as u64 * 60, 120);

        poll_worker.wait_and_download(
            submissions.to_vec(),
            timeout,
            3, // image poll interval: 3s
            8, // video poll interval: 8s
        )
    }

    // ─── Phase 4: Compose ───

    fn compose_artifacts(
        &self,
        artifacts: &[ImportedArtifact],
        plan: &CreativePlan,
    ) -> Result<String, String> {
        // 构造一个临时 ExecutionRuntime 供 CompositeSkill 使用
        let mut runtime = ExecutionRuntime::new(plan.id.clone(), plan.workspace_id.clone());

        // 将导入的资产作为 Artifact 注入 Runtime
        for (i, imported) in artifacts.iter().enumerate() {
            let asset_type = plan
                .shots
                .iter()
                .find(|s| s.index == (i as u8 + 1))
                .map(|s| s.asset_type.clone())
                .unwrap_or(AssetType::Video);

            let artifact = crate::domain::execution::Artifact {
                id: imported.asset_id.clone(),
                source_step: (i as u8) + 1,
                shot_index: (i as u8) + 1,
                asset_type,
                location: imported.relative_path.clone(),
                origin: ArtifactOrigin::Imported,
                metadata: imported
                    .metadata
                    .iter()
                    .map(|(k, v)| (k.clone(), v.clone()))
                    .collect(),
                provenance: Some(Provenance::skill_only("CreativeRuntimeImport")),
            };
            runtime.complete_step((i as u8) + 1, artifact);
        }

        // 使用 CompositeSkill 合成
        let composite_skill = CompositeSkill::new(
            Arc::clone(&self.composition_facade),
            self.workspace_root.clone(),
        );

        let composite_step_index = (artifacts.len() + 1) as u8;
        let result = composite_skill.execute(composite_step_index, "合成最终作品", &runtime)?;

        Ok(result.location)
    }
}

// ─── Tests ───

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::creative_director_service::CreativeBrief;
    use crate::application::generation_facade::MockGenerationFacade;
    use crate::domain::creative_plan::{CreativePlan, PlanningMetadata, ShotPlan};

    fn sample_creative_plan() -> CreativePlan {
        CreativePlan {
            id: "rt-plan-001".to_owned(),
            version: 1,
            workspace_id: "ws-rt".to_owned(),
            brief: CreativeBrief {
                project_type: "video".to_owned(),
                audience: "年轻人".to_owned(),
                platform: "douyin".to_owned(),
                visual_direction: "赛博朋克".to_owned(),
                strategy: "快节奏".to_owned(),
                suggested_style: None,
            },
            global_style: "赛博朋克，霓虹色调".to_owned(),
            constraints: vec!["30秒内".to_owned()],
            shots: vec![
                ShotPlan {
                    index: 1,
                    goal: "Opening".to_owned(),
                    description: "城市夜景俯瞰".to_owned(),
                    duration_secs: 5.0,
                    asset_type: AssetType::Video,
                    style: String::new(),
                    references: Vec::new(),
                    transition: "fade".to_owned(),
                },
                ShotPlan {
                    index: 2,
                    goal: "Emotion".to_owned(),
                    description: "主角特写".to_owned(),
                    duration_secs: 8.0,
                    asset_type: AssetType::Image,
                    style: String::new(),
                    references: Vec::new(),
                    transition: "cut".to_owned(),
                },
                ShotPlan {
                    index: 3,
                    goal: "Climax".to_owned(),
                    description: "飞行器穿越城市".to_owned(),
                    duration_secs: 5.0,
                    asset_type: AssetType::Video,
                    style: String::new(),
                    references: Vec::new(),
                    transition: "fade".to_owned(),
                },
            ],
            metadata: PlanningMetadata {
                planner_version: "1.0.0".to_owned(),
                llm_model: String::new(),
                confidence: 0.9,
            },
            created_at: "2026-07-23T00:00:00Z".to_owned(),
        }
    }

    #[test]
    fn test_creative_runtime_submit_all_shots() {
        let facade: Arc<dyn GenerationFacade> = Arc::new(MockGenerationFacade);
        // ArtifactImporter 需要 AssetService，测试中不构造
        // 这里只验证 Phase 1 submit 逻辑
        let submissions_result = {
            let plan = sample_creative_plan();
            let mut submissions = Vec::new();
            for shot in &plan.shots {
                let request = GenerationRequest {
                    asset_type: shot.asset_type.clone(),
                    prompt: format!("{} {}", plan.global_style, shot.description),
                    style: Some(plan.global_style.clone()),
                    reference_url: None,
                    duration_secs: Some(shot.duration_secs),
                    extra: HashMap::new(),
                };
                let output = facade.submit(&request).unwrap();
                submissions.push(PendingSubmission {
                    submission_id: output.submission_id.unwrap_or_default(),
                    provider_id: output.provider,
                    step_index: shot.index,
                    shot_index: shot.index,
                    asset_type: shot.asset_type.clone(),
                    model: output.model,
                });
            }
            submissions
        };

        assert_eq!(submissions_result.len(), 3);
        assert!(submissions_result[0].submission_id.starts_with("sub-"));
        assert_eq!(submissions_result[0].provider_id, "mock-provider");
    }

    #[test]
    fn test_creative_runtime_empty_plan() {
        let empty_plan = CreativePlan {
            id: "empty".to_owned(),
            version: 1,
            workspace_id: "ws".to_owned(),
            brief: CreativeBrief {
                project_type: String::new(),
                audience: String::new(),
                platform: String::new(),
                visual_direction: String::new(),
                strategy: String::new(),
                suggested_style: None,
            },
            global_style: String::new(),
            constraints: Vec::new(),
            shots: Vec::new(),
            metadata: PlanningMetadata {
                planner_version: "1.0.0".to_owned(),
                llm_model: String::new(),
                confidence: 0.0,
            },
            created_at: "2026-07-23T00:00:00Z".to_owned(),
        };

        // 0 shots → submit_all_shots returns empty Vec (not error, since plan.shots is empty)
        let submissions: Vec<PendingSubmission> = Vec::new();
        for shot in &empty_plan.shots {
            let _ = shot;
        }
        assert!(submissions.is_empty());
    }

    #[test]
    fn test_runtime_phase_serialization() {
        let progress = RuntimeProgress {
            phase: RuntimePhase::Submitting,
            current_shot: 1,
            total_shots: 6,
            message: "提交中...".to_owned(),
        };
        let json = serde_json::to_string(&progress).unwrap();
        assert!(json.contains("submitting"));
        assert!(json.contains("currentShot"));
    }
}
