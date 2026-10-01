//! 规划引擎：从 AgentService 提取的规划阶段逻辑。
//!
//! 职责：
//! - 构建规划上下文（用户系统提示词 + Creative Director + 创作状态 + 记忆）
//! - 调用 LLM 生成执行计划（含创作快速路径）
//! - 降级为 fallback 单步计划
//! - 检测直接回答（打招呼/闲聊）

use std::sync::{Arc, Mutex};

use crate::{
    application::{
        creative_director_service::{CreativeBrief, CreativeDirectorService},
        error::AppError,
    },
    domain::{
        agent::{
            parse_plan_from_text, ChatMessage, ChatRequest, ConversationRecord, PlanDraft,
            PlanRecord, PlanStep, PlanStepKind, PlanStepStatus, PLAN_MAX_ITERATIONS,
        },
        creative_plan::CreativePlan,
    },
    ports::{agent_llm::AgentLlm, plan_repository::PlanRepository},
};

use super::agent_service::AgentEvent;

/// 规划阶段产物：正常计划，或简单对话的直接回答。
#[allow(clippy::large_enum_variant)] // Planned 含完整 PlanRecord，DirectAnswer 路径无性能影响
pub(crate) enum PlanningOutcome {
    /// 生成了执行计划（通用规划或创作快速路径）。
    Planned {
        plan: PlanRecord,
        creative_plan: Option<CreativePlan>,
    },
    /// 简单对话（打招呼/闲聊/可直接回答的问题）：
    /// LLM 已在规划调用中给出简洁回答，无需创建计划、无需进入执行阶段。
    DirectAnswer { content: String },
}

/// 规划阶段的结构化上下文（替代字符串拼接）。
///
/// 各层信息独立存放，由 `render()` 统一渲染为最终 prompt。
/// 拼接顺序刻意按"最稳定 → 最易变"排列，提升供应商侧隐式前缀缓存命中率。
struct PlanningContext {
    /// 用户系统提示词（会话级）。
    system_prompt: Option<String>,
    /// Creative Director 分析结果（仅创作任务时有值）。
    creative_brief: Option<CreativeBrief>,
    /// 原始创作状态上下文（CreativeState 的 prompt context）。
    creative_state_context: Option<String>,
    /// 记忆检索结果。
    memory_context: Option<String>,
}

impl PlanningContext {
    /// 渲染为最终规划 prompt。
    fn render(&self) -> String {
        let base = "你是一位友好的 AIGC 创作助手，正在帮助学生完成创作任务。\n\n\
            首先判断学生的消息是否需要执行计划：\n\
            - 如果只是打招呼、闲聊，或不需要调用工具、不需要多步骤创作就能回答的简单问题，\
            请在第一行输出标记 [直接回答]，随后直接回答学生，不要输出计划。\n\
            - 其他情况需要先制定一个清晰的计划，再按计划执行。\n\n\
            回复长度硬性限制（必须遵守）：\n\
            - 直接回答最多 2 句话，能一句话说清就用一句。\n\
            - 禁止开场客套（如「好的！」「没问题！」「这是一个好问题！」）和结尾客套（如「希望对你有帮助」「祝创作愉快」）。\n\
            - 禁止复述或改写用户的问题，禁止展望式表态（如「接下来我可以…」）。\n\
            - 禁止使用 emoji、markdown 标题和分点列表（用户明确要求列举时除外）。\n\n\
            需要计划时，请按以下格式输出你的计划：\n\
            1. [步骤1的描述]\n\
            2. [步骤2的描述]\n\
            ...\n\n\
            每个步骤应该是具体的、可执行的行动。步骤数量宁少勿多：单一操作（例如修改图片里的某个元素）1 个步骤即可；简单请求 2-3 步；只有用户明确要求系列/多镜头/分镜内容时才用更多步骤，最多 8 个。\n\
            严禁扩大用户请求的范围：用户没有要求的额外镜头、变体、系列内容一律不要加入计划。\n\
            不要替用户回答或确认；需要用户决定时调用 ask_user_question 工具并停止等待。\n\
            计划描述要精炼，每步一句话即可。\n\
            用中文回复。";

        let mut prompt = base.to_owned();

        // 注入用户自定义系统提示词（会话级稳定，紧跟 base 之后）。
        if let Some(custom) = &self.system_prompt {
            if !custom.trim().is_empty() {
                prompt.push_str("\n\n");
                prompt.push_str(custom);
            }
        }

        // 注入创作状态（工作区级稳定）。
        if let Some(creative) = &self.creative_state_context {
            if !creative.is_empty() {
                prompt.push_str("\n\n[创作状态]\n");
                prompt.push_str(creative);
                prompt.push_str("\n请基于以上创作状态来制定计划，保持风格和角色的一致性。");
            }
        }

        // 注入 Creative Director 分析结果（轮次级）。
        if let Some(brief) = &self.creative_brief {
            prompt.push_str("\n\n[创作方向（Creative Director 分析）]");
            prompt.push_str(&format!("\n项目类型：{}", brief.project_type));
            if !brief.audience.is_empty() {
                prompt.push_str(&format!("\n目标受众：{}", brief.audience));
            }
            if !brief.platform.is_empty() {
                prompt.push_str(&format!("\n投放平台：{}", brief.platform));
            }
            if !brief.visual_direction.is_empty() {
                prompt.push_str(&format!("\n视觉方向：{}", brief.visual_direction));
            }
            if !brief.strategy.is_empty() {
                prompt.push_str(&format!("\n创作策略：{}", brief.strategy));
            }
        }

        // 注入记忆（最易变：随每次查询内容变化，放最末尾）。
        if let Some(mem) = &self.memory_context {
            if !mem.is_empty() {
                prompt.push_str("\n\n[相关记忆]\n");
                prompt.push_str(mem);
                prompt.push_str("\n请参考以上记忆来制定更贴合学生偏好的计划。");
            }
        }

        prompt
    }
}

/// 规划引擎：负责 Agent 对话的规划阶段。
///
/// 从 AgentService.run_planning_phase() + create_fallback_plan() 提取。
/// 通过 EventBus 发射 PlanCreated / DirectorFallback 事件。
pub(crate) struct PlanningEngine {
    plan_repository: Mutex<Box<dyn PlanRepository>>,
    creative_director: Option<Arc<CreativeDirectorService>>,
    creative_state_service:
        Option<Arc<crate::application::creative_state_service::CreativeStateService>>,
}

impl PlanningEngine {
    pub fn new(
        plan_repository: impl PlanRepository + 'static,
        creative_state_service: Option<
            Arc<crate::application::creative_state_service::CreativeStateService>,
        >,
    ) -> Self {
        Self {
            plan_repository: Mutex::new(Box::new(plan_repository)),
            creative_director: None,
            creative_state_service,
        }
    }

    pub fn with_creative_director(mut self, director: Arc<CreativeDirectorService>) -> Self {
        self.creative_director = Some(director);
        self
    }

    /// 规划阶段：调用 LLM 生成执行计划（不带工具）。
    ///
    /// 返回 `PlanningOutcome`：正常计划（`Planned`，创作快速路径时附带
    /// CreativePlan），或简单对话的直接回答（`DirectAnswer`）。
    #[allow(clippy::too_many_arguments)]
    pub fn run_planning_phase(
        &self,
        app: &tauri::AppHandle,
        conversation_id: &str,
        user_content: &str,
        raw_user_content: &str,
        has_image: bool,
        conversation: &ConversationRecord,
        llm: &mut dyn AgentLlm,
        model_name: &str,
        memory_context: Option<String>,
        image_data_urls: &[String],
    ) -> Result<PlanningOutcome, AppError> {
        // 加载创作状态上下文（始终加载，作为基础上下文）。
        let creative_state_context = self.creative_state_service.as_ref().and_then(|svc| {
            svc.get_prompt_context(&conversation.workspace_id)
                .ok()
                .flatten()
        });

        // ── 单镜头视频快速路径（P1 / 技术决策 D7）──
        // 非分镜的视频请求直接建 1 步视频任务：确定性、省掉 Director 与 LLM 规划
        // 两次调用，并从机制上杜绝"擅自加镜头/混入图片步骤"。
        // 分镜意图（多镜头）仍走 Director→Planner，由 P2 的分镜规划负责。
        if wants_video(raw_user_content) && !has_image && !is_storyboard_request(raw_user_content) {
            let secs = parse_video_duration_secs(raw_user_content);
            let steps = build_single_shot_video_steps(user_content, secs);
            let draft =
                PlanDraft::try_new(conversation_id.to_owned(), user_content.to_owned(), steps)?;
            let record = self.create_plan_from_draft(draft)?;
            super::agent_service::emit_event(
                app,
                conversation_id,
                AgentEvent::PlanCreated {
                    conversation_id: conversation_id.to_owned(),
                    plan: record.clone(),
                },
            );
            eprintln!("[Planner] single-shot video fast path: 1 step");
            return Ok(PlanningOutcome::Planned {
                plan: record,
                creative_plan: None,
            });
        }

        // ── 单张图片快速路径（D7 镜像）──
        // 非分镜的图片请求直接建 1 步图片任务：Director 倾向把单图请求展开成
        // 多镜头多步计划（22:22 复盘：单图被规划成 5 步，同一个提交失败在
        // 每一步各刷一条回复）。确定性单步同时消除过度规划与重复回复。
        if wants_image(raw_user_content) && !has_image && !is_storyboard_request(raw_user_content) {
            let steps = build_single_shot_image_steps(user_content);
            let draft =
                PlanDraft::try_new(conversation_id.to_owned(), user_content.to_owned(), steps)?;
            let record = self.create_plan_from_draft(draft)?;
            super::agent_service::emit_event(
                app,
                conversation_id,
                AgentEvent::PlanCreated {
                    conversation_id: conversation_id.to_owned(),
                    plan: record.clone(),
                },
            );
            eprintln!("[Planner] single-shot image fast path: 1 step");
            return Ok(PlanningOutcome::Planned {
                plan: record,
                creative_plan: None,
            });
        }

        // Creative Director 分析：仅对创作类任务调用。
        // 注意用原始用户文本判定：图片分析注入的增强文本充满"海报/设计"等词，
        // 会让"去除二维码"这类简单修改请求被误判为创作任务而过度展开多镜头计划。
        let creative_brief = if is_creative_task(raw_user_content, has_image) {
            if let Some(director) = self.creative_director.as_ref() {
                match director.analyze_and_direct(
                    &conversation.workspace_id,
                    user_content,
                    model_name,
                    llm,
                ) {
                    Ok(brief) => {
                        eprintln!(
                            "[Director] brief generated: project_type={}",
                            brief.project_type
                        );
                        Some(brief)
                    }
                    Err(e) => {
                        eprintln!("[Director] analysis failed, falling back: {e}");
                        super::agent_service::emit_event(
                            app,
                            conversation_id,
                            AgentEvent::DirectorFallback {
                                conversation_id: conversation_id.to_owned(),
                                reason: e.to_string(),
                            },
                        );
                        None
                    }
                }
            } else {
                None
            }
        } else {
            eprintln!("[Director] skipped (non-creative task)");
            None
        };

        // ── 创作任务快速路径：Director → Planner → PlanBuilder → PlanStep[] ──
        if let Some(brief) = &creative_brief {
            let planner =
                crate::application::creative_planner_service::CreativePlannerService::new();
            match planner.plan(
                &conversation.workspace_id,
                brief,
                user_content,
                model_name,
                llm,
            ) {
                Ok(creative_plan) => {
                    let exec_plan = crate::application::creative_plan_builder::build_execution_plan(
                        &creative_plan,
                    );
                    eprintln!(
                        "[Planner] plan generated: {} shots, {:.0}s total",
                        creative_plan.shot_count(),
                        creative_plan.total_duration_secs()
                    );
                    let draft = PlanDraft::try_new(
                        conversation_id.to_owned(),
                        user_content.to_owned(),
                        exec_plan.steps,
                    )?;
                    let record = self.create_plan_from_draft(draft)?;
                    super::agent_service::emit_event(
                        app,
                        conversation_id,
                        AgentEvent::PlanCreated {
                            conversation_id: conversation_id.to_owned(),
                            plan: record.clone(),
                        },
                    );
                    return Ok(PlanningOutcome::Planned {
                        plan: record,
                        creative_plan: Some(creative_plan),
                    });
                }
                Err(e) => {
                    eprintln!("[Planner] failed, falling back to LLM planning: {e}");
                    // Fall through to generic LLM planning below.
                }
            }
        }

        // ── 通用路径：LLM 规划（非创作任务，或 Planner 失败时的降级）──
        let planning_context = PlanningContext {
            system_prompt: conversation.system_prompt.clone(),
            creative_brief,
            creative_state_context,
            memory_context,
        };
        let planning_prompt = planning_context.render();
        let mut plan_record: Option<PlanRecord> = None;

        for attempt in 0..PLAN_MAX_ITERATIONS {
            let user_msg = if image_data_urls.is_empty() {
                ChatMessage::user(user_content.to_owned())
            } else {
                ChatMessage::user_with_images(user_content.to_owned(), image_data_urls.to_vec())
            };
            // 用户上传了图片时，限制计划只做图片操作（不生成视频）
            let mut planning_prompt = planning_prompt.clone();
            if has_image {
                planning_prompt.push_str(
                    "
[当前任务限制]
用户上传了图片并要求修改图片。只创建图片生成步骤，不要创建视频生成步骤。
",
                );
            }
            // 视频意图约束（D7，镜像 has_image 做法）：覆盖 Director/Planner 失败
            // 降级到通用规划的场景，防止视频请求被规划成图片步骤或擅自加镜头。
            if wants_video(raw_user_content) && !has_image {
                planning_prompt.push_str(
                    "
[当前任务限制]
用户要求生成视频。只创建视频生成步骤（video_generation），不要创建图片生成步骤。
单个视频请求只创建 1 个步骤，不要擅自增加镜头数。
调用 video_generation 工具时通过 durationSeconds 传递时长（当前供应商支持 5 或 10 秒）。
",
                );
            }
            let chat_messages = vec![ChatMessage::system(planning_prompt.clone()), user_msg];
            let request = ChatRequest::new(
                model_name.to_owned(),
                chat_messages,
                Vec::new(), // no tools in planning phase
            );

            let response = match llm.chat(&request) {
                Ok(r) => r,
                Err(e) => {
                    eprintln!("[Planner] LLM call failed (attempt {}): {e}", attempt + 1);
                    if attempt == PLAN_MAX_ITERATIONS - 1 {
                        let fallback =
                            self.create_fallback_plan(app, conversation_id, user_content)?;
                        return Ok(PlanningOutcome::Planned {
                            plan: fallback,
                            creative_plan: None,
                        });
                    }
                    continue;
                }
            };

            if let Some(content) = &response.content {
                let preview: String = content.chars().take(100).collect();
                eprintln!(
                    "[Planner] LLM response ({} chars): {}",
                    content.len(),
                    preview
                );
                if let Some(answer) = extract_direct_answer(content) {
                    eprintln!("[Planner] direct answer detected, skipping plan");
                    return Ok(PlanningOutcome::DirectAnswer { content: answer });
                }
                let steps = parse_plan_from_text(content);
                eprintln!("[Planner] parsed {} steps from response", steps.len());
                if !steps.is_empty() {
                    let draft = match PlanDraft::try_new(
                        conversation_id.to_owned(),
                        user_content.to_owned(),
                        steps,
                    ) {
                        Ok(d) => d,
                        Err(e) => {
                            eprintln!("[Planner] PlanDraft::try_new failed: {e}");
                            return Err(AppError::from(e));
                        }
                    };
                    let record = match self.create_plan_from_draft(draft) {
                        Ok(r) => r,
                        Err(e) => {
                            eprintln!("[Planner] create_plan_from_draft failed: {e}");
                            return Err(e);
                        }
                    };
                    super::agent_service::emit_event(
                        app,
                        conversation_id,
                        AgentEvent::PlanCreated {
                            conversation_id: conversation_id.to_owned(),
                            plan: record.clone(),
                        },
                    );
                    plan_record = Some(record);
                    break;
                }
            } else {
                eprintln!("[Planner] LLM response has no content");
            }
        }

        match plan_record {
            Some(record) => Ok(PlanningOutcome::Planned {
                plan: record,
                creative_plan: None,
            }),
            None => {
                eprintln!(
                    "[Planner] no parseable steps after {PLAN_MAX_ITERATIONS} attempts, using fallback plan"
                );
                self.create_fallback_plan(app, conversation_id, user_content)
                    .map(|plan| PlanningOutcome::Planned {
                        plan,
                        creative_plan: None,
                    })
            }
        }
    }

    /// 创建降级计划（规划失败时使用）。
    fn create_fallback_plan(
        &self,
        app: &tauri::AppHandle,
        conversation_id: &str,
        user_content: &str,
    ) -> Result<PlanRecord, AppError> {
        let steps = vec![PlanStep {
            index: 1,
            description: format!("执行：{user_content}"),
            status: PlanStepStatus::Pending,
            kind: crate::domain::agent::PlanStepKind::Task,
        }];
        let draft = PlanDraft::try_new(conversation_id.to_owned(), user_content.to_owned(), steps)?;
        let record = self.create_plan_from_draft(draft)?;
        super::agent_service::emit_event(
            app,
            conversation_id,
            AgentEvent::PlanCreated {
                conversation_id: conversation_id.to_owned(),
                plan: record.clone(),
            },
        );
        Ok(record)
    }

    fn create_plan_from_draft(&self, draft: PlanDraft) -> Result<PlanRecord, AppError> {
        let mut repo = self
            .plan_repository
            .lock()
            .map_err(|_| AppError::StateUnavailable)?;
        repo.create_plan(draft).map_err(AppError::from)
    }

    /// 通过 PlanRepository 更新步骤状态（供 ExecutionEngine 调用）。
    pub fn update_plan_step(
        &self,
        plan_id: &str,
        step_index: u8,
        status: PlanStepStatus,
    ) -> Result<PlanRecord, AppError> {
        let mut repo = self
            .plan_repository
            .lock()
            .map_err(|_| AppError::StateUnavailable)?;
        repo.update_plan_step(plan_id, step_index, status)
            .map_err(AppError::from)
    }
}

/// 判断用户消息是否为创作类任务（决定是否调用 Creative Director）。
///
/// 关键词匹配：包含创作相关动词/名词时视为创作任务。
pub(crate) fn is_creative_task(message: &str, has_image: bool) -> bool {
    // 用户上传了图片 → 大概率是图片操作任务，直接走创作路径。
    if has_image {
        return true;
    }
    const CREATIVE_KEYWORDS: &[&str] = &[
        "制作",
        "生成",
        "设计",
        "创作",
        "绘制",
        "画",
        "视频",
        "图片",
        "海报",
        "动画",
        "宣传片",
        "短片",
        "角色",
        "分镜",
        "漫画",
        "插画",
        "封面",
        "logo",
        "剪辑",
        "配音",
        "特效",
        "渲染",
        "排版",
        "做一个",
        "帮我做",
        "我想要一个",
        "给我生成",
    ];
    let lower = message.to_lowercase();
    CREATIVE_KEYWORDS.iter().any(|kw| lower.contains(kw))
}

/// 判断用户消息是否要求生成视频（区别于图片等其它创作物）。
pub(crate) fn wants_video(message: &str) -> bool {
    const VIDEO_KEYWORDS: &[&str] = &["视频", "动画", "宣传片", "短片", "影片", "动起来", "mv"];
    let lower = message.to_lowercase();
    VIDEO_KEYWORDS.iter().any(|kw| lower.contains(kw))
}

/// 判断是否为分镜/多镜头意图：这类请求保留给 Director→Planner 出多镜头计划（P2）。
pub(crate) fn is_storyboard_request(message: &str) -> bool {
    const STORYBOARD_KEYWORDS: &[&str] = &[
        "分镜",
        "镜头",
        "几段",
        "分段",
        "拆分",
        "依次",
        "画面切换",
        "场景切换",
    ];
    STORYBOARD_KEYWORDS.iter().any(|kw| message.contains(kw))
}

/// 从消息中解析视频时长（秒），支持"5秒"/"5 秒"/"5s"；未提及返回 None。
///
/// 数字后必须紧跟单位（秒/s），避免把"5个镜头"误判为时长；
/// 当前主流视频供应商（kling/seedance）单镜头支持 5/10 秒，钳制到该范围，
/// 避免把不支持的时长提交到远端后被整单拒绝。
pub(crate) fn parse_video_duration_secs(message: &str) -> Option<f32> {
    let compact: String = message
        .to_lowercase()
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect();
    let chars: Vec<char> = compact.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if !chars[i].is_ascii_digit() {
            i += 1;
            continue;
        }
        let start = i;
        while i < chars.len() && chars[i].is_ascii_digit() {
            i += 1;
        }
        // 数字后紧跟单位才视为时长
        if matches!(chars.get(i), Some('秒') | Some('s')) {
            let digits: String = chars[start..i].iter().collect();
            if let Ok(value) = digits.parse::<f32>() {
                return Some(value.clamp(1.0, 10.0));
            }
        }
    }
    None
}

/// 构造单镜头视频任务的执行步骤（快速路径，纯函数便于测试）。
///
/// 描述里显式携带时长，执行阶段的 LLM 会把它转成 video_generation
/// 工具的 durationSeconds 参数。
pub(crate) fn build_single_shot_video_steps(
    prompt: &str,
    duration_secs: Option<f32>,
) -> Vec<PlanStep> {
    let secs = duration_secs.unwrap_or(5.0);
    vec![PlanStep {
        index: 1,
        description: format!("生成视频（{secs}秒）— {prompt}"),
        status: PlanStepStatus::Pending,
        kind: PlanStepKind::VideoGeneration,
    }]
}

/// 判断用户消息是否要求生成图片（区别于视频等其它创作物）。
pub(crate) fn wants_image(message: &str) -> bool {
    const IMAGE_KEYWORDS: &[&str] = &[
        "画", "图", "图片", "海报", "插画", "照片", "壁纸", "头像", "封面",
    ];
    if wants_video(message) {
        return false;
    }
    let lower = message.to_lowercase();
    IMAGE_KEYWORDS.iter().any(|kw| lower.contains(kw))
}

/// 构造单张图片任务的执行步骤（快速路径，纯函数便于测试）。
pub(crate) fn build_single_shot_image_steps(prompt: &str) -> Vec<PlanStep> {
    vec![PlanStep {
        index: 1,
        description: format!("生成图片 — {prompt}"),
        status: PlanStepStatus::Pending,
        kind: PlanStepKind::ImageGeneration,
    }]
}

/// 检测规划响应是否为"直接回答"格式并提取回答内容。
///
/// 约定：模型在首行输出 `[直接回答]` 标记，其后（可带全/半角冒号）为给学生的回答。
/// 标记后内容为空视为无效，返回 None 走正常规划重试。
pub(crate) fn extract_direct_answer(content: &str) -> Option<String> {
    let rest = content.trim_start().strip_prefix("[直接回答]")?;
    let answer = rest.trim_start_matches('：').trim_start_matches(':').trim();
    if answer.is_empty() {
        None
    } else {
        Some(answer.to_owned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::agent::PlanStepKind;

    #[test]
    fn test_wants_video_detection() {
        assert!(wants_video("生成一段5秒的海浪视频"));
        assert!(wants_video("做一支产品宣传片"));
        assert!(wants_video("把这张图动起来")); // 无图片上下文时仅作意图判断
        assert!(wants_video("来个 MG 动画"));
        assert!(!wants_video("画一张海报"));
        assert!(!wants_video("生成一只猫的图片"));
    }

    #[test]
    fn test_storyboard_request_detection() {
        // 分镜意图走 Director→Planner（P2），不进单镜头快速路径
        assert!(is_storyboard_request("做一支30秒的宣传片，3个镜头"));
        assert!(is_storyboard_request("按分镜脚本生成视频"));
        assert!(is_storyboard_request("拆成几段画面"));
        assert!(!is_storyboard_request("生成一段5秒的海浪视频"));
    }

    #[test]
    fn test_parse_video_duration_secs() {
        assert_eq!(
            parse_video_duration_secs("生成一段5秒的海浪视频"),
            Some(5.0)
        );
        assert_eq!(parse_video_duration_secs("生成 10 s 的视频"), Some(10.0));
        assert_eq!(parse_video_duration_secs("5秒钟的浪潮"), Some(5.0));
        // 无单位不当作时长（"5个镜头"是分镜信号不是时长）
        assert_eq!(parse_video_duration_secs("做3个镜头的宣传片"), None);
        // 超出供应商支持范围时钳制
        assert_eq!(parse_video_duration_secs("生成30秒的视频"), Some(10.0));
        assert_eq!(parse_video_duration_secs("0秒"), Some(1.0));
        assert_eq!(parse_video_duration_secs("没有时长要求"), None);
    }

    #[test]
    fn test_build_single_shot_video_steps() {
        let steps = build_single_shot_video_steps("生成一段海浪视频", Some(5.0));
        assert_eq!(steps.len(), 1);
        assert_eq!(steps[0].kind, PlanStepKind::VideoGeneration);
        assert_eq!(steps[0].index, 1);
        assert!(steps[0].description.contains("5秒"));
        assert!(steps[0].description.contains("海浪"));

        // 未提及时长时缺省 5 秒
        let default_steps = build_single_shot_video_steps("海浪", None);
        assert!(default_steps[0].description.contains("5秒"));
    }

    #[test]
    fn test_wants_image_detection() {
        assert!(wants_image("画一张海浪拍打礁石的图"));
        assert!(wants_image("生成一张海浪拍打礁石的写实摄影照片"));
        assert!(wants_image("做一张游戏海报"));
        assert!(wants_image("帮我画一张头像"));
        // 视频请求优先走视频快速路径
        assert!(!wants_image("生成一段海浪视频"));
        assert!(!wants_image("做一支产品宣传片"));
        // 与视频无关的普通对话不误判
        assert!(!wants_image("今天天气怎么样"));
    }

    #[test]
    fn test_build_single_shot_image_steps() {
        let steps = build_single_shot_image_steps("海浪拍打礁石的写实摄影照片");
        assert_eq!(steps.len(), 1);
        assert_eq!(steps[0].kind, PlanStepKind::ImageGeneration);
        assert_eq!(steps[0].index, 1);
        assert!(steps[0].description.contains("海浪"));
    }
}
