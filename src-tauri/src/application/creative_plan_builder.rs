//! Creative Plan Builder：将 CreativePlan 转换为 Agent 可执行的 ExecutionPlan。
//!
//! 职责：解耦 Planner 与 Agent。Agent 不知道 CreativePlan 的存在，
//! 只看到标准的 ExecutionPlan（steps + 未来可扩展字段）。

use crate::domain::{
    agent::{PlanStep, PlanStepKind, PlanStepStatus},
    creative_plan::{AssetType, CreativePlan},
};

/// 执行计划（PlanBuilder 的输出，Agent 的输入）。
///
/// 当前只包含 steps，未来可扩展 retry_policy / parallel_groups / variables。
#[derive(Debug, Clone)]
pub struct ExecutionPlan {
    /// 有序执行步骤。
    pub steps: Vec<PlanStep>,
}

/// 将 CreativePlan 转换为 ExecutionPlan。
///
/// 每个 ShotPlan 生成一个 PlanStep，描述中包含镜头目标、资产类型和时长，
/// 让 Agent 的 ReAct 循环能理解需要调用什么工具。
pub fn build_execution_plan(plan: &CreativePlan) -> ExecutionPlan {
    let mut steps: Vec<PlanStep> = Vec::new();
    let mut index: u8 = 1;

    // 如果有全局风格约束，作为第一个步骤（设定风格基调）。
    if !plan.global_style.is_empty() || !plan.constraints.is_empty() {
        let mut desc = String::from("确定创作风格基调");
        if !plan.global_style.is_empty() {
            desc.push_str(&format!("：{}", plan.global_style));
        }
        if !plan.constraints.is_empty() {
            desc.push_str(&format!("（约束：{}）", plan.constraints.join("、")));
        }
        steps.push(PlanStep {
            index,
            description: desc,
            status: PlanStepStatus::Pending,
            kind: PlanStepKind::StyleSetup,
        });
        index += 1;
    }

    // 每个镜头生成一个步骤。
    for shot in &plan.shots {
        let asset_label = match shot.asset_type {
            AssetType::Image => "图片",
            AssetType::Video => "视频",
            AssetType::Audio => "音频",
            AssetType::Text => "文案",
        };

        let kind = match shot.asset_type {
            AssetType::Image => PlanStepKind::ImageGeneration,
            AssetType::Video => PlanStepKind::VideoGeneration,
            AssetType::Audio => PlanStepKind::AudioGeneration,
            AssetType::Text => PlanStepKind::Task,
        };

        let description = format!(
            "镜头{} [{}]：生成{}（{}秒）— {}",
            shot.index, shot.goal, asset_label, shot.duration_secs, shot.description
        );

        steps.push(PlanStep {
            index,
            description,
            status: PlanStepStatus::Pending,
            kind,
        });
        index += 1;
    }

    // 如果有多于 1 个视频/图片镜头，添加合成步骤。
    let visual_shots = plan
        .shots
        .iter()
        .filter(|s| s.asset_type == AssetType::Video || s.asset_type == AssetType::Image)
        .count();
    if visual_shots > 1 {
        steps.push(PlanStep {
            index,
            description: format!(
                "将{}个视觉素材按顺序合成为完整作品（总时长约{:.0}秒）",
                visual_shots,
                plan.total_duration_secs()
            ),
            status: PlanStepStatus::Pending,
            kind: PlanStepKind::Composite,
        });
    }

    ExecutionPlan { steps }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::creative_director_service::CreativeBrief;
    use crate::domain::creative_plan::PlanningMetadata;

    fn sample_plan() -> CreativePlan {
        CreativePlan {
            id: "test-plan".to_owned(),
            version: 1,
            workspace_id: "ws-1".to_owned(),
            brief: CreativeBrief {
                project_type: "video".to_owned(),
                audience: "年轻人".to_owned(),
                platform: "douyin".to_owned(),
                visual_direction: "赛博朋克".to_owned(),
                strategy: "快节奏剪辑".to_owned(),
                suggested_style: None,
            },
            global_style: "赛博朋克，霓虹色调".to_owned(),
            constraints: vec!["时长不超过30秒".to_owned()],
            shots: vec![
                crate::domain::creative_plan::ShotPlan {
                    index: 1,
                    goal: "Opening".to_owned(),
                    description: "城市夜景俯瞰".to_owned(),
                    duration_secs: 5.0,
                    asset_type: AssetType::Video,
                    style: String::new(),
                    references: Vec::new(),
                    transition: "fade".to_owned(),
                },
                crate::domain::creative_plan::ShotPlan {
                    index: 2,
                    goal: "Emotion".to_owned(),
                    description: "主角行走在霓虹街道".to_owned(),
                    duration_secs: 8.0,
                    asset_type: AssetType::Video,
                    style: String::new(),
                    references: Vec::new(),
                    transition: "cut".to_owned(),
                },
            ],
            metadata: PlanningMetadata {
                planner_version: "1.0.0".to_owned(),
                llm_model: String::new(),
                confidence: 0.9,
            },
            created_at: "2026-07-22T00:00:00Z".to_owned(),
        }
    }

    #[test]
    fn test_build_plan_steps_basic() {
        let plan = sample_plan();
        let exec_plan = build_execution_plan(&plan);
        let steps = &exec_plan.steps;

        // 1 style step + 2 shot steps + 1 composite step = 4
        assert_eq!(steps.len(), 4);
        assert_eq!(steps[0].index, 1);
        assert!(steps[0].description.contains("风格基调"));
        assert!(steps[1].description.contains("镜头1"));
        assert!(steps[1].description.contains("Opening"));
        assert!(steps[2].description.contains("镜头2"));
        assert!(steps[3].description.contains("合成"));
    }

    #[test]
    fn test_build_plan_steps_no_style() {
        let mut plan = sample_plan();
        plan.global_style = String::new();
        plan.constraints = Vec::new();

        let exec_plan = build_execution_plan(&plan);
        let steps = &exec_plan.steps;
        // No style step: 2 shots + 1 composite = 3
        assert_eq!(steps.len(), 3);
        assert_eq!(steps[0].index, 1);
        assert!(steps[0].description.contains("镜头1"));
    }

    #[test]
    fn test_build_plan_steps_single_shot_no_composite() {
        let mut plan = sample_plan();
        plan.global_style = String::new();
        plan.constraints = Vec::new();
        plan.shots.truncate(1);

        let exec_plan = build_execution_plan(&plan);
        let steps = &exec_plan.steps;
        // 1 shot, no composite (only 1 visual shot)
        assert_eq!(steps.len(), 1);
        assert!(!steps[0].description.contains("合成"));
    }
}
