use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use serde::Serialize;
use tauri::{AppHandle, Emitter};

use crate::{
    adapters::providers::grok_chat::GrokChatAdapter,
    application::{
        creative_director_service::{CreativeBrief, CreativeDirectorService},
        error::AppError,
        generation_pipeline::GenerationPipeline,
        generation_submit_service::GenerationSubmitService,
        provider_registry::ProviderRegistry,
        response_cache::ResponseCache,
    },
    domain::{
        agent::{
            normalize_conversation_title, parse_plan_from_text, ChatMessage, ChatRequest,
            ConversationDraft, ConversationRecord, ConversationStatus, MessageDraft, MessageRecord,
            MessageRole, PlanDraft, PlanRecord, PlanStep, PlanStepStatus, ToolCall,
            ToolInvocationDraft, ToolInvocationRecord, ToolInvocationStatus,
            EXECUTE_MAX_ITERATIONS, PLAN_MAX_ITERATIONS,
        },
        creative_plan::CreativePlan,
    },
    ports::{
        agent_llm::{AgentLlm, AgentLlmError},
        agent_repository::{AgentRepository, AgentRepositoryError},
        agent_tool_executor::{AgentToolError, AgentToolExecutor, ToolContext},
        credential_repository::CredentialRepository,
        memory_service::MemoryServicePort,
        plan_repository::PlanRepository,
        reloadable::Reloadable,
    },
};

/// ReAct 工具循环事件，通过 Tauri event 推送给前端。
/// 前端通过 `listen('agent://event', ...)` 接收。
#[derive(Debug, Clone, Serialize)]
// rename_all 只改变体名（kind 标签）；结构体变体内部字段需 rename_all_fields
// 才能序列化为 camelCase，否则前端 Zod schema 会因 conversation_id/chunk_index
// 等 snake_case 字段拒收全部实时事件。
#[serde(
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    tag = "kind"
)]
pub(crate) enum AgentEvent {
    /// 一条新消息已写入（user/assistant/tool）。
    MessageAppended {
        conversation_id: String,
        message: MessageRecord,
    },
    /// 工具调用状态变更（pending → running → succeeded/failed）。
    ToolInvocationUpdated {
        conversation_id: String,
        invocation: ToolInvocationRecord,
    },
    /// 循环正常结束。
    Done {
        conversation_id: String,
        finish_reason: String,
    },
    /// 循环异常终止。
    Failed {
        conversation_id: String,
        error: String,
    },
    /// 计划已创建（规划阶段完成）。
    PlanCreated {
        conversation_id: String,
        plan: PlanRecord,
    },
    /// 计划已更新（步骤状态变化）。
    #[allow(dead_code)]
    PlanUpdated {
        conversation_id: String,
        plan: PlanRecord,
    },
    /// 计划步骤状态变化。
    PlanStepChanged {
        conversation_id: String,
        plan_id: String,
        step_index: u8,
        status: PlanStepStatus,
    },
    /// Agent 向学生提问（中断执行，等待回答）。
    UserQuestionAsked {
        conversation_id: String,
        tool_call_id: String,
        question: String,
    },
    /// 规划阶段检索到的相关记忆。
    MemoryRecalled {
        conversation_id: String,
        memories: Vec<MemoryRecallEntry>,
    },
    /// 流式内容片段（实时输出）。
    StreamChunk {
        conversation_id: String,
        chunk: String,
        chunk_index: u32,
    },
    /// 流式输出完成。
    StreamDone {
        conversation_id: String,
        full_content: String,
        total_chunks: u32,
    },
    /// Creative Director 分析失败，降级为原始创作状态路径。
    DirectorFallback {
        conversation_id: String,
        reason: String,
    },
}

/// 记忆检索条目（发送给前端展示）。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct MemoryRecallEntry {
    pub source_type: String,
    pub content: String,
    pub score: f64,
}

/// 发送消息后的最终结果（用于 IPC 返回）。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SendMessageResult {
    pub conversation_id: String,
    /// 最后一条 assistant 消息（finish_reason="stop" 时）或最后一条 assistant/tool 消息。
    pub final_message: Option<MessageRecord>,
    /// 整个循环产生的所有工具调用记录。
    pub invocations: Vec<ToolInvocationRecord>,
    /// 结束原因：stop / tool_calls / length / content_filter / max_iterations / error
    pub finish_reason: String,
}

/// 规划阶段的结构化上下文（替代字符串拼接）。
///
/// 各层信息独立存放，由 `render()` 统一渲染为最终 prompt。
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
    ///
    /// 拼接顺序刻意按"最稳定 → 最易变"排列：
    /// base（恒定）→ 用户系统提示词（会话级稳定）→ 创作状态（工作区级稳定）
    /// → Creative Director 分析（创作轮次级）→ 记忆（随每次查询变化）。
    /// 同一会话连续调用 LLM 时相同前缀尽可能长，
    /// 提升供应商侧隐式前缀缓存（prompt cache）的命中率。
    fn render(&self) -> String {
        let base = "你是一位友好的 AIGC 创作助手，正在帮助学生完成创作任务。\n\n\
            首先判断学生的消息是否需要执行计划：\n\
            - 如果只是打招呼、闲聊，或不需要调用工具、不需要多步骤创作就能回答的简单问题，\
            请在第一行输出标记 [直接回答]，随后用简洁自然的语言直接回答学生（一两句话即可，不要冗长），不要输出计划。\n\
            - 其他情况需要先制定一个清晰的计划，再按计划执行。\n\n\
            需要计划时，请按以下格式输出你的计划：\n\
            1. [步骤1的描述]\n\
            2. [步骤2的描述]\n\
            ...\n\n\
            每个步骤应该是具体的、可执行的行动。步骤数量控制在 3-8 个。\n\
            如果学生的请求比较简单，2-3 个步骤即可。\n\
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

/// 判断用户消息是否为创作类任务（决定是否调用 Creative Director）。
///
/// 关键词匹配：包含创作相关动词/名词时视为创作任务。
fn is_creative_task(message: &str, has_image: bool) -> bool {
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

/// 规划阶段产物：正常计划，或简单对话的直接回答。
enum PlanningOutcome {
    /// 生成了执行计划（通用规划或创作快速路径）。
    Planned {
        plan: PlanRecord,
        creative_plan: Option<CreativePlan>,
    },
    /// 简单对话（打招呼/闲聊/可直接回答的问题）：
    /// LLM 已在规划调用中给出简洁回答，无需创建计划、无需进入执行阶段。
    DirectAnswer { content: String },
}

/// 检测规划响应是否为"直接回答"格式并提取回答内容。
///
/// 约定：模型在首行输出 `[直接回答]` 标记，其后（可带全/半角冒号）为给学生的回答。
/// 标记后内容为空视为无效，返回 None 走正常规划重试。
fn extract_direct_answer(content: &str) -> Option<String> {
    let rest = content.trim_start().strip_prefix("[直接回答]")?;
    let answer = rest.trim_start_matches('：').trim_start_matches(':').trim();
    if answer.is_empty() {
        None
    } else {
        Some(answer.to_owned())
    }
}

pub struct AgentService {
    agent_repository: Mutex<Box<dyn AgentRepository>>,
    credential_repository: Mutex<Box<dyn CredentialRepository>>,
    tool_executor: Mutex<Box<dyn AgentToolExecutor>>,
    plan_repository: Mutex<Box<dyn PlanRepository>>,
    memory_service: Option<Box<dyn MemoryServicePort>>,
    creative_state_service:
        Option<std::sync::Arc<crate::application::creative_state_service::CreativeStateService>>,
    creative_director: Option<Arc<CreativeDirectorService>>,
    creative_runtime:
        Option<Arc<crate::application::creative_runtime_service::CreativeRuntimeService>>,
    generation_provider_registry: Option<Arc<ProviderRegistry>>,
    generation_pipeline: Option<Arc<GenerationPipeline>>,
    database_path: PathBuf,
    /// 轻量语义响应缓存：直接回答类消息命中后免 LLM 调用。
    response_cache: ResponseCache,
    /// 用户选择的生成输出目录（可选），由前端通过 IPC 设置。
    output_directory: std::sync::Mutex<Option<PathBuf>>,
    /// Agent 运行时：拆分后的核心编排器。设置后 send_message 委托给它。
    agent_runtime: std::sync::Mutex<Option<super::agent_runtime::AgentRuntime>>,
}

impl AgentService {
    pub fn new(
        agent_repository: impl AgentRepository + 'static,
        credential_repository: impl CredentialRepository + 'static,
        tool_executor: impl AgentToolExecutor + 'static,
        plan_repository: impl PlanRepository + 'static,
        memory_service: Option<impl MemoryServicePort + 'static>,
        creative_state_service: Option<
            std::sync::Arc<crate::application::creative_state_service::CreativeStateService>,
        >,
        database_path: PathBuf,
    ) -> Self {
        Self {
            agent_repository: Mutex::new(Box::new(agent_repository)),
            credential_repository: Mutex::new(Box::new(credential_repository)),
            tool_executor: Mutex::new(Box::new(tool_executor)),
            plan_repository: Mutex::new(Box::new(plan_repository)),
            memory_service: memory_service.map(|m| Box::new(m) as Box<dyn MemoryServicePort>),
            creative_state_service,
            creative_director: None,
            creative_runtime: None,
            generation_provider_registry: None,
            generation_pipeline: None,
            database_path,
            response_cache: ResponseCache::new(),
            output_directory: std::sync::Mutex::new(None),
            agent_runtime: std::sync::Mutex::new(None),
        }
    }

    pub fn with_generation_engine(
        mut self,
        provider_registry: Arc<ProviderRegistry>,
        pipeline: Arc<GenerationPipeline>,
    ) -> Self {
        self.generation_provider_registry = Some(provider_registry);
        self.generation_pipeline = Some(pipeline);
        self
    }

    /// 设置生成输出目录。前端在用户选择目录后调用。
    pub fn set_output_directory(&self, path: Option<PathBuf>) {
        // 透传给 AgentRuntime。
        if let Ok(rt_guard) = self.agent_runtime.lock() {
            if let Some(rt) = rt_guard.as_ref() {
                rt.set_output_directory(path.clone());
            }
        }
        if let Ok(mut dir) = self.output_directory.lock() {
            *dir = path;
        }
    }

    pub fn with_creative_director(mut self, director: Arc<CreativeDirectorService>) -> Self {
        self.creative_director = Some(director);
        self
    }

    pub fn with_creative_runtime(
        mut self,
        runtime: Arc<crate::application::creative_runtime_service::CreativeRuntimeService>,
    ) -> Self {
        self.creative_runtime = Some(runtime);
        self
    }

    /// 注入 Agent 运行时（拆分后的核心编排器）。
    /// 设置后，send_message 将委托给 AgentRuntime 处理。
    pub fn with_agent_runtime(self, runtime: super::agent_runtime::AgentRuntime) -> Self {
        if let Ok(mut rt) = self.agent_runtime.lock() {
            *rt = Some(runtime);
        }
        self
    }

    pub fn create_conversation(
        &self,
        workspace_id: String,
        title: String,
        credential_id: String,
        system_prompt: Option<String>,
    ) -> Result<ConversationRecord, AppError> {
        let draft = ConversationDraft::try_new(workspace_id, title, credential_id, system_prompt)?;
        self.with_agent_repository(|repo| repo.create_conversation(draft).map_err(AppError::from))
    }

    pub fn list_conversations(
        &self,
        workspace_id: &str,
    ) -> Result<Vec<ConversationRecord>, AppError> {
        self.with_agent_repository(|repo| {
            repo.list_conversations(workspace_id)
                .map_err(AppError::from)
        })
    }

    pub fn get_conversation(
        &self,
        conversation_id: &str,
    ) -> Result<Option<ConversationRecord>, AppError> {
        self.with_agent_repository(|repo| {
            repo.get_conversation(conversation_id)
                .map_err(AppError::from)
        })
    }

    pub fn delete_conversation(&self, conversation_id: &str) -> Result<(), AppError> {
        self.with_agent_repository(|repo| {
            repo.delete_conversation(conversation_id)
                .map_err(AppError::from)
        })
    }

    /// 重命名会话标题。标题规则与创建会话一致（非空、≤200 字符）。
    pub fn rename_conversation(
        &self,
        conversation_id: &str,
        title: String,
    ) -> Result<ConversationRecord, AppError> {
        let title = normalize_conversation_title(title)?;
        self.with_agent_repository(|repo| {
            repo.rename_conversation(conversation_id, &title)
                .map_err(AppError::from)
        })
    }

    pub fn list_messages(&self, conversation_id: &str) -> Result<Vec<MessageRecord>, AppError> {
        let messages = self.with_agent_repository(|repo| {
            repo.list_messages(conversation_id).map_err(AppError::from)
        })?;
        // 诊断日志：打印每条用户消息的内容长度和是否包含图片
        for msg in &messages {
            if msg.role == crate::domain::agent::MessageRole::User {
                let (len, has_images) = match &msg.content {
                    Some(c) => (
                        c.len(),
                        c.starts_with(r#"{"images":"#) || c.starts_with(r#"{"text":"#),
                    ),
                    None => (0, false),
                };
                eprintln!(
                    "[Agent] list_messages user_msg id={} content_len={} has_images={}",
                    msg.id, len, has_images
                );
            }
        }
        Ok(messages)
    }

    pub fn list_invocations(
        &self,
        conversation_id: &str,
    ) -> Result<Vec<ToolInvocationRecord>, AppError> {
        self.with_agent_repository(|repo| {
            repo.list_invocations(conversation_id)
                .map_err(AppError::from)
        })
    }

    /// 发送用户消息并运行 Plan-and-Execute 循环。
    ///
    /// 流程：
    /// 1. 持久化用户消息
    /// 2. 规划阶段：LLM 生成编号计划 → 持久化 → 推送 PlanCreated
    /// 3. 执行阶段：逐步执行计划，每步运行工具循环
    /// 4. 如果工具为 ask_user_question，中断循环等待用户回答
    pub fn send_message(
        &self,
        app: &AppHandle,
        conversation_id: &str,
        user_content: String,
        image_data_urls: Vec<String>,
    ) -> Result<SendMessageResult, AppError> {
        // 委托给 AgentRuntime（拆分后的核心编排器）。
        if let Ok(rt_guard) = self.agent_runtime.lock() {
            if let Some(rt) = rt_guard.as_ref() {
                return rt.send_message(app, conversation_id, user_content, image_data_urls);
            }
        }

        eprintln!(
            "[Agent] send_message conv={} content_len={} attachments={} first_data_url_len={}",
            conversation_id,
            user_content.len(),
            image_data_urls.len(),
            image_data_urls.first().map(|u| u.len()).unwrap_or(0)
        );
        // 1. 持久化用户消息（有附件时存为 JSON 多模态格式）。
        let stored_content = if image_data_urls.is_empty() {
            user_content.clone()
        } else {
            let json = serde_json::json!({
                "images": image_data_urls,
                "text": user_content,
            });
            let s = json.to_string();
            eprintln!(
                "[Agent] stored_content_len={} starts_with={:?}",
                s.len(),
                &s[..s.len().min(50)]
            );
            s
        };
        let user_message = self.with_agent_repository(|repo| {
            repo.append_message(MessageDraft::user(
                conversation_id.to_owned(),
                stored_content,
            )?)
            .map_err(AppError::from)
        })?;
        emit_event(
            app,
            conversation_id,
            AgentEvent::MessageAppended {
                conversation_id: conversation_id.to_owned(),
                message: user_message.clone(),
            },
        );

        // 2. 加载会话 + 凭据，构造 LLM 适配器。
        let conversation = self.with_agent_repository(|repo| {
            repo.get_conversation(conversation_id)?
                .ok_or_else(|| {
                    AgentRepositoryError::ConversationNotFound(conversation_id.to_owned())
                })
                .map_err(AppError::from)
        })?;

        // 语义响应缓存：直接回答类消息（打招呼/闲聊）重复出现时，
        // 直接返回缓存回答，跳过凭据加载与 LLM 调用。
        if let Some(cached) = self
            .response_cache
            .get(&user_content, conversation.system_prompt.as_deref())
        {
            eprintln!(
                "[Agent] response cache hit ({} chars), skipping LLM",
                cached.len()
            );
            let assistant_message = self.with_agent_repository(|repo| {
                repo.append_message(MessageDraft::assistant(
                    conversation_id.to_owned(),
                    Some(cached),
                    Vec::new(),                             // tool_calls
                    None,                                   // remote_model
                    Some("direct_answer_cache".to_owned()), // finish_reason
                    None,                                   // prompt_tokens
                    None,                                   // completion_tokens
                )?)
                .map_err(AppError::from)
            })?;
            emit_event(
                app,
                conversation_id,
                AgentEvent::MessageAppended {
                    conversation_id: conversation_id.to_owned(),
                    message: assistant_message.clone(),
                },
            );
            emit_event(
                app,
                conversation_id,
                AgentEvent::Done {
                    conversation_id: conversation_id.to_owned(),
                    finish_reason: "stop".to_owned(),
                },
            );
            return Ok(SendMessageResult {
                conversation_id: conversation_id.to_owned(),
                final_message: Some(assistant_message),
                invocations: Vec::new(),
                finish_reason: "stop".to_owned(),
            });
        }

        let credential = self.with_credential_repository(|repo| {
            repo.get(&conversation.credential_id)?.ok_or_else(|| {
                AppError::from(
                    crate::ports::credential_repository::CredentialRepositoryError::NotFound(
                        conversation.credential_id.clone(),
                    ),
                )
            })
        })?;
        let api_key = self.with_credential_repository(|repo| {
            repo.get_secret(&credential.credential_key)
                .map_err(AppError::from)
        })?;
        let mut llm = GrokChatAdapter::new(credential.base_url.clone(), api_key);

        // 3. 加载工具定义。
        let tools = self.with_tool_executor(|executor| Ok(executor.list_tools()))?;

        // 4. 记忆召回只做一次，结果同时注入规划与执行阶段提示词。
        let (memory_context, memory_entries) =
            self.recall_memory_with_entries(&conversation.workspace_id, &user_content);
        if !memory_entries.is_empty() {
            emit_event(
                app,
                conversation_id,
                AgentEvent::MemoryRecalled {
                    conversation_id: conversation_id.to_owned(),
                    memories: memory_entries,
                },
            );
        }

        // ── 规划阶段 ──
        let outcome = self.run_planning_phase(
            app,
            conversation_id,
            &user_content,
            &user_content,
            !image_data_urls.is_empty(),
            &conversation,
            &mut llm,
            &credential.model_name,
            memory_context.clone(),
            &image_data_urls,
        )?;

        // 简单对话快速路径：打招呼/闲聊等无需计划的消息，
        // 直接落库规划阶段得到的简洁回答，不创建计划、不进入执行阶段。
        let (plan, creative_plan) = match outcome {
            PlanningOutcome::DirectAnswer { content } => {
                eprintln!("[Agent] direct answer fast path: {} chars", content.len());
                // 写入语义缓存：下次相同消息直接命中，免 LLM 调用。
                self.response_cache.put(
                    &user_content,
                    conversation.system_prompt.as_deref(),
                    &content,
                );
                let assistant_message = self.with_agent_repository(|repo| {
                    repo.append_message(MessageDraft::assistant(
                        conversation_id.to_owned(),
                        Some(content),
                        Vec::new(),                       // tool_calls
                        None,                             // remote_model
                        Some("direct_answer".to_owned()), // finish_reason
                        None,                             // prompt_tokens
                        None,                             // completion_tokens
                    )?)
                    .map_err(AppError::from)
                })?;
                emit_event(
                    app,
                    conversation_id,
                    AgentEvent::MessageAppended {
                        conversation_id: conversation_id.to_owned(),
                        message: assistant_message.clone(),
                    },
                );
                emit_event(
                    app,
                    conversation_id,
                    AgentEvent::Done {
                        conversation_id: conversation_id.to_owned(),
                        finish_reason: "stop".to_owned(),
                    },
                );
                return Ok(SendMessageResult {
                    conversation_id: conversation_id.to_owned(),
                    final_message: Some(assistant_message),
                    invocations: Vec::new(),
                    finish_reason: "stop".to_owned(),
                });
            }
            PlanningOutcome::Planned {
                plan,
                creative_plan,
            } => (plan, creative_plan),
        };

        // ── 执行阶段 ──
        // 创作快速路径：当 CreativePlan 存在且 CreativeRuntimeService 可用时，
        // 替换 ReAct 循环，直接驱动四阶段流水线。
        if let (Some(rt), Some(cp)) = (&self.creative_runtime, &creative_plan) {
            eprintln!(
                "[Agent] creative runtime fast path: {} shots",
                cp.shot_count()
            );
            let result = rt.execute_plan(cp);
            let summary_text = format_runtime_result(&result);
            let assistant_message = self.with_agent_repository(|repo| {
                repo.append_message(MessageDraft::assistant(
                    conversation_id.to_owned(),
                    Some(summary_text),
                    Vec::new(),                          // tool_calls
                    None,                                // remote_model
                    Some("creative_runtime".to_owned()), // finish_reason
                    None,                                // prompt_tokens
                    None,                                // completion_tokens
                )?)
                .map_err(AppError::from)
            })?;
            emit_event(
                app,
                conversation_id,
                AgentEvent::MessageAppended {
                    conversation_id: conversation_id.to_owned(),
                    message: assistant_message.clone(),
                },
            );
            return Ok(SendMessageResult {
                conversation_id: conversation_id.to_owned(),
                final_message: Some(assistant_message),
                invocations: Vec::new(),
                finish_reason: match result.status {
                    crate::domain::execution::ExecutionStatus::Completed => "stop".to_owned(),
                    crate::domain::execution::ExecutionStatus::Partial => "partial".to_owned(),
                    _ => "error".to_owned(),
                },
            });
        }

        // 通用路径：LLM ReAct 循环
        self.run_execution_phase(
            app,
            conversation_id,
            &plan,
            &conversation,
            &mut llm,
            &tools,
            &credential.model_name,
            memory_context.as_deref(),
        )
    }

    /// 规划阶段：调用 LLM 生成执行计划（不带工具）。
    ///
    /// 返回 `PlanningOutcome`：正常计划（`Planned`，创作快速路径时附带
    /// CreativePlan），或简单对话的直接回答（`DirectAnswer`）。
    fn run_planning_phase(
        &self,
        app: &AppHandle,
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
        // 记忆召回已上提到 send_message（一次召回，规划/执行共用），
        // 此处仅消费传入的 memory_context。

        // 加载创作状态上下文（始终加载，作为基础上下文）。
        let creative_state_context = self.creative_state_service.as_ref().and_then(|svc| {
            svc.get_prompt_context(&conversation.workspace_id)
                .ok()
                .flatten()
        });

        // Creative Director 分析：仅对创作类任务调用（用原始文本判定，见 planning_engine）。
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
                        emit_event(
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
                    let record = self.with_plan_repository(|repo| {
                        repo.create_plan(draft).map_err(AppError::from)
                    })?;
                    emit_event(
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
        // 构建结构化规划上下文并渲染为 prompt。
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
                        // 规划失败，降级为单步执行
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

            // 解析 LLM 响应为计划。
            if let Some(content) = &response.content {
                // 简单对话（打招呼/闲聊）：模型以 [直接回答] 标记给出简洁回答，
                // 跳过计划创建与执行阶段，避免多余的重试和冗长回复。
                if let Some(answer) = extract_direct_answer(content) {
                    eprintln!("[Planner] direct answer detected, skipping plan");
                    return Ok(PlanningOutcome::DirectAnswer { content: answer });
                }
                let steps = parse_plan_from_text(content);
                if !steps.is_empty() {
                    let draft = PlanDraft::try_new(
                        conversation_id.to_owned(),
                        user_content.to_owned(),
                        steps,
                    )?;
                    let record = self.with_plan_repository(|repo| {
                        repo.create_plan(draft).map_err(AppError::from)
                    })?;
                    emit_event(
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
            }
        }

        match plan_record {
            Some(record) => Ok(PlanningOutcome::Planned {
                plan: record,
                creative_plan: None,
            }),
            None => {
                // LLM 正常应答但没有可解析的计划步骤（如简单追问被直接回答），
                // 降级为单步计划；同样需要推送 PlanCreated 事件。
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
        app: &AppHandle,
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
        let record =
            self.with_plan_repository(|repo| repo.create_plan(draft).map_err(AppError::from))?;
        // 降级计划同样要推送前端，否则本轮执行计划面板不会显示。
        emit_event(
            app,
            conversation_id,
            AgentEvent::PlanCreated {
                conversation_id: conversation_id.to_owned(),
                plan: record.clone(),
            },
        );
        Ok(record)
    }

    /// 执行阶段：逐步执行计划中的每个步骤。
    fn run_execution_phase(
        &self,
        app: &AppHandle,
        conversation_id: &str,
        plan: &PlanRecord,
        conversation: &ConversationRecord,
        llm: &mut dyn AgentLlm,
        tools: &[crate::domain::agent::ToolDefinition],
        model_name: &str,
        memory_context: Option<&str>,
    ) -> Result<SendMessageResult, AppError> {
        let mut total_iterations = 0u8;
        let mut last_assistant: Option<MessageRecord> = None;
        let mut last_finish_reason = "stop".to_owned();
        let mut all_invocations: Vec<ToolInvocationRecord> = Vec::new();
        let mut current_plan = plan.clone();

        for step in &current_plan.steps.clone() {
            if total_iterations >= EXECUTE_MAX_ITERATIONS {
                break;
            }

            // 更新步骤状态为 in-progress。
            current_plan = self.with_plan_repository(|repo| {
                repo.update_plan_step(&plan.id, step.index, PlanStepStatus::InProgress)
                    .map_err(AppError::from)
            })?;
            emit_event(
                app,
                conversation_id,
                AgentEvent::PlanStepChanged {
                    conversation_id: conversation_id.to_owned(),
                    plan_id: plan.id.clone(),
                    step_index: step.index,
                    status: PlanStepStatus::InProgress,
                },
            );

            // 构建执行阶段系统提示词（静态前缀 + 动态尾部，含记忆注入）。
            let exec_prompt = build_execution_system_prompt(
                &plan.goal,
                step,
                &current_plan.steps,
                memory_context,
            );

            // 执行当前步骤的工具循环。
            let step_result = self.execute_single_step(
                app,
                conversation_id,
                &exec_prompt,
                conversation,
                llm,
                tools,
                model_name,
                &mut total_iterations,
                &mut last_assistant,
                &mut last_finish_reason,
                &mut all_invocations,
            );

            match step_result {
                Ok(()) => {
                    // 步骤完成。
                    #[allow(unused_assignments)]
                    {
                        current_plan = self.with_plan_repository(|repo| {
                            repo.update_plan_step(&plan.id, step.index, PlanStepStatus::Completed)
                                .map_err(AppError::from)
                        })?;
                    }
                    emit_event(
                        app,
                        conversation_id,
                        AgentEvent::PlanStepChanged {
                            conversation_id: conversation_id.to_owned(),
                            plan_id: plan.id.clone(),
                            step_index: step.index,
                            status: PlanStepStatus::Completed,
                        },
                    );
                }
                Err(AppError::AgentUserQuestionPending) => {
                    // 用户问题已发射事件，中断执行。
                    return Ok(SendMessageResult {
                        conversation_id: conversation_id.to_owned(),
                        final_message: last_assistant,
                        invocations: all_invocations,
                        finish_reason: "user_question".to_owned(),
                    });
                }
                Err(_e) => {
                    // 步骤失败。
                    self.with_plan_repository(|repo| {
                        repo.update_plan_step(&plan.id, step.index, PlanStepStatus::Failed)
                            .map_err(AppError::from)
                    })?;
                    emit_event(
                        app,
                        conversation_id,
                        AgentEvent::PlanStepChanged {
                            conversation_id: conversation_id.to_owned(),
                            plan_id: plan.id.clone(),
                            step_index: step.index,
                            status: PlanStepStatus::Failed,
                        },
                    );
                    // 继续执行下一步，不中断整个计划
                }
            }

            if total_iterations >= EXECUTE_MAX_ITERATIONS {
                break;
            }
        }

        // 对话完成后存储到长期记忆。
        if let Ok(history) = self.with_agent_repository(|repo| {
            repo.list_messages(conversation_id).map_err(AppError::from)
        }) {
            self.store_memory(&conversation.workspace_id, conversation_id, &history);
        }

        // 发送完成事件。
        emit_event(
            app,
            conversation_id,
            AgentEvent::Done {
                conversation_id: conversation_id.to_owned(),
                finish_reason: last_finish_reason.clone(),
            },
        );

        Ok(SendMessageResult {
            conversation_id: conversation_id.to_owned(),
            final_message: last_assistant,
            invocations: all_invocations,
            finish_reason: last_finish_reason,
        })
    }

    /// 执行单个计划步骤的工具循环。
    fn execute_single_step(
        &self,
        app: &AppHandle,
        conversation_id: &str,
        system_prompt: &str,
        conversation: &ConversationRecord,
        llm: &mut dyn AgentLlm,
        tools: &[crate::domain::agent::ToolDefinition],
        model_name: &str,
        iterations: &mut u8,
        last_assistant: &mut Option<MessageRecord>,
        last_finish_reason: &mut String,
        all_invocations: &mut Vec<ToolInvocationRecord>,
    ) -> Result<(), AppError> {
        // 流式片段序号计数器（Cell 以便在 Fn 回调内部递增）。
        let stream_chunk_index = std::cell::Cell::new(0u32);
        loop {
            *iterations += 1;
            if *iterations > EXECUTE_MAX_ITERATIONS {
                return Err(AppError::AgentLoopTooManyIterations(EXECUTE_MAX_ITERATIONS));
            }

            // 加载历史消息，构建滑动窗口。
            let history = self.with_agent_repository(|repo| {
                repo.list_messages(conversation_id).map_err(AppError::from)
            })?;
            let chat_messages = build_chat_messages_with_window(
                system_prompt,
                &history,
                20, // 默认滑动窗口
            );
            let request = ChatRequest::new(model_name.to_owned(), chat_messages, tools.to_vec());

            // 调用 LLM（流式）：每个文本增量片段通过 StreamChunk 事件实时推送给前端。
            let response = match llm.chat_stream(&request, &|chunk| {
                let index = stream_chunk_index.get();
                stream_chunk_index.set(index.saturating_add(1));
                emit_event(
                    app,
                    conversation_id,
                    AgentEvent::StreamChunk {
                        conversation_id: conversation_id.to_owned(),
                        chunk: chunk.to_owned(),
                        chunk_index: index,
                    },
                );
            }) {
                Ok(r) => r,
                Err(AgentLlmError::Remote { status, body }) => {
                    let detail = extract_remote_error_detail(&body);
                    let hint = match status {
                        402 => "。请在「模型管理」中检查该凭据的 API Key 是否有可用余额，或切换到其他有余额的凭据。",
                        401 => "。API Key 无效，请在「模型管理」中重新配置。",
                        429 => "。请求过于频繁，请稍后重试。",
                        _ => "",
                    };
                    let error_msg = format!("LLM 请求失败 (HTTP {status})：{detail}{hint}");
                    self.mark_conversation_error(conversation_id, &error_msg)?;
                    emit_event(
                        app,
                        conversation_id,
                        AgentEvent::Failed {
                            conversation_id: conversation_id.to_owned(),
                            error: error_msg.clone(),
                        },
                    );
                    return Err(AppError::AgentLlmError(error_msg));
                }
                Err(other) => {
                    let error_msg = other.to_string();
                    self.mark_conversation_error(conversation_id, &error_msg)?;
                    emit_event(
                        app,
                        conversation_id,
                        AgentEvent::Failed {
                            conversation_id: conversation_id.to_owned(),
                            error: error_msg.clone(),
                        },
                    );
                    return Err(AppError::AgentLlmError(error_msg));
                }
            };

            // 持久化 assistant 消息。
            let assistant_record = self.with_agent_repository(|repo| {
                repo.append_message(MessageDraft::assistant(
                    conversation_id.to_owned(),
                    response.content.clone(),
                    response.tool_calls.clone(),
                    Some(response.model.clone()),
                    Some(response.finish_reason.clone()),
                    response.prompt_tokens,
                    response.completion_tokens,
                )?)
                .map_err(AppError::from)
            })?;
            emit_event(
                app,
                conversation_id,
                AgentEvent::MessageAppended {
                    conversation_id: conversation_id.to_owned(),
                    message: assistant_record.clone(),
                },
            );
            *last_assistant = Some(assistant_record);
            *last_finish_reason = response.finish_reason.clone();

            // 若没有工具调用，步骤完成。
            // 兼容不同 LLM：部分模型（如 MiMo）返回 finish_reason="stop" 但仍有 tool_calls，
            // 因此仅在 tool_calls 为空时才退出，不依赖 finish_reason 判断。
            if response.tool_calls.is_empty() {
                // 发射 StreamDone，通知前端结束流式展示（最终消息已通过 messageAppended 落库）。
                if let Some(full) = &response.content {
                    emit_event(
                        app,
                        conversation_id,
                        AgentEvent::StreamDone {
                            conversation_id: conversation_id.to_owned(),
                            full_content: full.clone(),
                            total_chunks: stream_chunk_index.get(),
                        },
                    );
                }
                return Ok(());
            }

            // 逐个执行工具调用。
            for tool_call in &response.tool_calls {
                // 检查是否为 ask_user_question 工具。
                if tool_call.function.name == "ask_user_question" {
                    let question = extract_question_from_args(&tool_call.function.arguments);
                    // 创建 pending invocation 记录。
                    let invocation = self.with_agent_repository(|repo| {
                        repo.create_invocation(ToolInvocationDraft::try_new(
                            last_assistant
                                .as_ref()
                                .map(|m| m.id.clone())
                                .unwrap_or_default(),
                            conversation_id.to_owned(),
                            "ask_user_question".to_owned(),
                            tool_call.function.arguments.clone(),
                        )?)
                        .map_err(AppError::from)
                    })?;
                    emit_event(
                        app,
                        conversation_id,
                        AgentEvent::ToolInvocationUpdated {
                            conversation_id: conversation_id.to_owned(),
                            invocation: invocation.clone(),
                        },
                    );
                    // 发射用户问题事件，中断循环。
                    // LLM 可能返回空 tool_call.id，兜底生成 UUID。
                    let safe_tool_call_id = if tool_call.id.trim().is_empty() {
                        uuid::Uuid::new_v4().to_string()
                    } else {
                        tool_call.id.clone()
                    };
                    emit_event(
                        app,
                        conversation_id,
                        AgentEvent::UserQuestionAsked {
                            conversation_id: conversation_id.to_owned(),
                            tool_call_id: safe_tool_call_id,
                            question,
                        },
                    );
                    return Err(AppError::AgentUserQuestionPending);
                }

                let invocation_record = self.execute_single_tool(
                    app,
                    conversation_id,
                    &last_assistant
                        .as_ref()
                        .map(|m| m.id.clone())
                        .unwrap_or_default(),
                    tool_call,
                    &conversation.workspace_id,
                )?;
                all_invocations.push(invocation_record);
            }
        }
    }

    fn execute_single_tool(
        &self,
        app: &AppHandle,
        conversation_id: &str,
        message_id: &str,
        tool_call: &ToolCall,
        workspace_id: &str,
    ) -> Result<ToolInvocationRecord, AppError> {
        // 1. 创建 pending 调用记录。
        let invocation = self.with_agent_repository(|repo| {
            repo.create_invocation(ToolInvocationDraft::try_new(
                message_id.to_owned(),
                conversation_id.to_owned(),
                tool_call.function.name.clone(),
                tool_call.function.arguments.clone(),
            )?)
            .map_err(AppError::from)
        })?;
        emit_event(
            app,
            conversation_id,
            AgentEvent::ToolInvocationUpdated {
                conversation_id: conversation_id.to_owned(),
                invocation: invocation.clone(),
            },
        );

        // 2. 标记 running。
        let invocation = self.update_invocation(
            &invocation.id,
            conversation_id,
            ToolInvocationStatus::Running,
            None,
            None,
            None,
        )?;
        emit_event(
            app,
            conversation_id,
            AgentEvent::ToolInvocationUpdated {
                conversation_id: conversation_id.to_owned(),
                invocation: invocation.clone(),
            },
        );

        // 3. 执行工具。
        let workspace_path = self
            .database_path
            .parent()
            .unwrap_or_else(|| std::path::Path::new("."))
            .to_path_buf();
        let output_directory = self
            .output_directory
            .lock()
            .ok()
            .and_then(|dir| dir.clone());
        let ctx = ToolContext {
            workspace_id: workspace_id.to_owned(),
            workspace_path,
            output_directory,
            conversation_id: conversation_id.to_owned(),
            sandbox: None,
            latest_user_image: None,
        };
        eprintln!(
            "[AgentTool] execute start name={} conv={conversation_id}",
            tool_call.function.name
        );
        let execution_result = self.with_tool_executor(|executor| {
            executor.execute(
                &ctx,
                &tool_call.function.name,
                &tool_call.function.arguments,
            )
        });
        let (status, result_json, error_message, generation_task_id) = match execution_result {
            Ok(result) => {
                eprintln!(
                    "[AgentTool] execute ok name={} result_len={}",
                    tool_call.function.name,
                    result.content.chars().count()
                );
                (
                    ToolInvocationStatus::Succeeded,
                    Some(result.content),
                    None,
                    result.generation_task_id,
                )
            }
            Err(error) => {
                let reason = error.to_string();
                eprintln!(
                    "[AgentTool] execute failed name={} err={reason}",
                    tool_call.function.name
                );
                (
                    ToolInvocationStatus::Failed,
                    None,
                    Some(reason.clone()),
                    None,
                )
            }
        };

        // 4. 更新调用记录为 succeeded/failed。
        let updated_invocation = self.update_invocation(
            &invocation.id,
            conversation_id,
            status,
            result_json.clone(),
            error_message,
            generation_task_id,
        )?;
        emit_event(
            app,
            conversation_id,
            AgentEvent::ToolInvocationUpdated {
                conversation_id: conversation_id.to_owned(),
                invocation: updated_invocation.clone(),
            },
        );

        // 5. 写入 tool 消息回传给 LLM。
        let tool_message_content = result_json.unwrap_or_else(|| {
            serde_json::json!({
                "status": "error",
                "error": "tool execution failed"
            })
            .to_string()
        });
        let tool_record = self.with_agent_repository(|repo| {
            repo.append_message(MessageDraft::tool_message(
                conversation_id.to_owned(),
                tool_call.id.clone(),
                tool_message_content,
            )?)
            .map_err(AppError::from)
        })?;
        emit_event(
            app,
            conversation_id,
            AgentEvent::MessageAppended {
                conversation_id: conversation_id.to_owned(),
                message: tool_record,
            },
        );

        Ok(updated_invocation)
    }

    fn update_invocation(
        &self,
        invocation_id: &str,
        conversation_id: &str,
        status: ToolInvocationStatus,
        result_json: Option<String>,
        error_message: Option<String>,
        generation_task_id: Option<String>,
    ) -> Result<ToolInvocationRecord, AppError> {
        self.with_agent_repository(|repo| {
            repo.update_invocation_status(
                invocation_id,
                status,
                result_json,
                error_message,
                generation_task_id,
            )
            .map_err(AppError::from)
        })?;
        // 重新查询返回最新记录。
        let invocations = self.with_agent_repository(|repo| {
            repo.list_invocations(conversation_id)
                .map_err(AppError::from)
        })?;
        invocations
            .into_iter()
            .find(|i| i.id == invocation_id)
            .ok_or_else(|| {
                AgentRepositoryError::InvocationNotFound(invocation_id.to_owned()).into()
            })
    }

    fn mark_conversation_error(
        &self,
        conversation_id: &str,
        _message: &str,
    ) -> Result<(), AppError> {
        // 目前只更新状态，错误消息通过事件流推送；后续可考虑写到会话字段。
        self.with_agent_repository(|repo| {
            repo.update_conversation_status(conversation_id, ConversationStatus::Error)
                .map_err(AppError::from)
        })?;
        Ok(())
    }

    fn with_agent_repository<T>(
        &self,
        operation: impl FnOnce(&mut dyn AgentRepository) -> Result<T, AppError>,
    ) -> Result<T, AppError> {
        let mut repository = self
            .agent_repository
            .lock()
            .map_err(|_| AppError::StateUnavailable)?;
        operation(repository.as_mut())
    }

    fn with_credential_repository<T>(
        &self,
        operation: impl FnOnce(&mut dyn CredentialRepository) -> Result<T, AppError>,
    ) -> Result<T, AppError> {
        let mut repository = self
            .credential_repository
            .lock()
            .map_err(|_| AppError::StateUnavailable)?;
        operation(repository.as_mut())
    }

    fn with_tool_executor<T>(
        &self,
        operation: impl FnOnce(&mut dyn AgentToolExecutor) -> Result<T, AgentToolError>,
    ) -> Result<T, AgentToolError> {
        let mut executor =
            self.tool_executor
                .lock()
                .map_err(|_| AgentToolError::ExecutionFailed {
                    tool: "agent_loop".to_owned(),
                    reason: "executor lock unavailable".to_owned(),
                })?;
        operation(executor.as_mut())
    }

    fn with_plan_repository<T>(
        &self,
        operation: impl FnOnce(&mut dyn PlanRepository) -> Result<T, AppError>,
    ) -> Result<T, AppError> {
        let mut repository = self
            .plan_repository
            .lock()
            .map_err(|_| AppError::StateUnavailable)?;
        operation(repository.as_mut())
    }

    /// 从长期记忆中检索相关内容，返回格式化文本和原始条目。
    fn recall_memory_with_entries(
        &self,
        workspace_id: &str,
        query: &str,
    ) -> (Option<String>, Vec<MemoryRecallEntry>) {
        let memory = match self.memory_service.as_ref() {
            Some(m) if m.is_available() => m,
            _ => return (None, Vec::new()),
        };
        let results = match memory.recall(workspace_id, query, 5) {
            Ok(r) if !r.is_empty() => r,
            _ => return (None, Vec::new()),
        };

        let entries: Vec<MemoryRecallEntry> = results
            .iter()
            .map(|r| MemoryRecallEntry {
                source_type: r.source_type.clone(),
                content: r.content.clone(),
                score: r.score,
            })
            .collect();

        let lines: Vec<String> = results
            .iter()
            .map(|r| {
                let prefix = match r.source_type.as_str() {
                    "episode" => "来自对话",
                    "fact" => "已知事实",
                    "profile" => "学生画像",
                    _ => "记忆",
                };
                format!("- ({prefix}) {}", r.content)
            })
            .collect();

        (Some(lines.join("\n")), entries)
    }

    /// 对话完成后存储到长期记忆。
    fn store_memory(&self, workspace_id: &str, conversation_id: &str, messages: &[MessageRecord]) {
        if let Some(memory) = &self.memory_service {
            if memory.is_available() {
                let _ = memory.remember_conversation(workspace_id, conversation_id, messages);
            }
        }
    }
}

/// 构建规划阶段系统提示词（带记忆上下文）。
#[cfg(test)]
fn build_planning_system_prompt_with_memory(
    user_system_prompt: &Option<String>,
    memory_context: Option<&str>,
    creative_context: Option<&str>,
) -> String {
    let base = "你是一位友好的 AIGC 创作助手，正在帮助学生完成创作任务。\n\n\
        在执行学生的请求之前，你需要先制定一个清晰的计划。\n\n\
        请按以下格式输出你的计划：\n\
        1. [步骤1的描述]\n\
        2. [步骤2的描述]\n\
        ...\n\n\
        每个步骤应该是具体的、可执行的行动。步骤数量控制在 3-8 个。\n\
        如果学生的请求比较简单，2-3 个步骤即可。\n\
        用中文回复。";

    let mut prompt = base.to_owned();

    if let Some(creative) = creative_context {
        if !creative.is_empty() {
            prompt.push_str("\n\n");
            prompt.push_str(creative);
            prompt.push_str("\n\n请基于以上创作状态来制定计划，保持风格和角色的一致性。");
        }
    }

    if let Some(mem) = memory_context {
        if !mem.is_empty() {
            prompt.push_str("\n\n[相关记忆]\n");
            prompt.push_str(mem);
            prompt.push_str("\n\n请参考以上记忆来制定更贴合学生偏好的计划。");
        }
    }

    if let Some(custom) = user_system_prompt {
        if !custom.trim().is_empty() {
            prompt.push_str("\n\n");
            prompt.push_str(custom);
        }
    }

    prompt
}

/// 执行阶段静态提示词前缀。
///
/// 提示词缓存关键：该文本在所有执行调用（跨步骤/跨轮次/跨会话）中保持完全一致，
/// 且位于 system 消息开头，供应商侧的隐式前缀缓存即可跨步骤命中，
/// 避免重复前缀被重新计费/重算。所有动态内容（目标/计划/当前步骤/记忆）
/// 一律拼接在该前缀之后，禁止在其前面插入任何内容。
const EXECUTION_STATIC_PREFIX: &str =
    "你是一位友好的 AIGC 创作助手，正在帮助学生完成创作任务。\n\n\
    执行规则：\n\
    - 专注完成当前步骤，不要提前执行后续步骤。\n\
    - 如果需要学生提供信息，使用 ask_user_question 工具提问。\n\
    - 需要生成图片时，必须调用 image_generation 工具，不要只描述图片。\n\
    - 需要生成视频时，必须调用 video_generation 工具。\n\
    - 调用工具后根据返回结果继续推进。\n\
    - 用中文回复，告诉学生你正在做什么以及进展。\n\n\
    可用工具：\n\
    - image_generation(prompt): 生成图片，传入详细的图片描述提示词\n\
    - video_generation(prompt): 生成视频，传入详细的视频描述提示词\n\
    - current_time(): 获取当前时间\n\
    - ask_user_question(question): 向用户提问";

/// 构建执行阶段系统提示词。
///
/// 结构：静态前缀（利于前缀缓存）+ 动态尾部（目标/计划/当前步骤/记忆）。
fn build_execution_system_prompt(
    goal: &str,
    current_step: &PlanStep,
    all_steps: &[PlanStep],
    memory_context: Option<&str>,
) -> String {
    let steps_list: String = all_steps
        .iter()
        .map(|s| {
            let status_icon = match s.status {
                PlanStepStatus::Completed => "✓",
                PlanStepStatus::InProgress => "→",
                PlanStepStatus::Failed => "✗",
                PlanStepStatus::Skipped => "⊘",
                _ => " ",
            };
            format!("{} {}. {}", status_icon, s.index, s.description)
        })
        .collect::<Vec<_>>()
        .join("\n");

    let mut prompt = EXECUTION_STATIC_PREFIX.to_owned();
    prompt.push_str(&format!("\n\n[当前目标]\n{goal}"));
    prompt.push_str(&format!("\n\n[执行计划]\n{steps_list}"));
    prompt.push_str(&format!(
        "\n\n[当前步骤]\n第 {} 步：{}",
        current_step.index, current_step.description
    ));

    // 注入长期记忆（放在最末尾，避免破坏前面的稳定前缀）。
    if let Some(mem) = memory_context {
        if !mem.trim().is_empty() {
            prompt.push_str("\n\n[相关记忆]\n");
            prompt.push_str(mem);
            prompt.push_str("\n请在执行时参考以上记忆，贴合学生偏好。");
        }
    }

    prompt
}

/// 从 ask_user_question 工具参数中提取问题文本。
/// 从远端 API 的错误响应体中提取可读的错误描述。
///
/// 尝试解析 JSON（如 `{"error":{"message":"..."}}`），失败时返回原始 body 的前 200 字符。
fn extract_remote_error_detail(body: &str) -> String {
    // 常见 OpenAI 兼容格式: {"error":{"message":"...","type":"...","code":"..."}}
    if let Ok(v) = serde_json::from_str::<serde_json::Value>(body) {
        if let Some(msg) = v
            .get("error")
            .and_then(|e| e.get("message"))
            .and_then(|m| m.as_str())
        {
            let code = v
                .get("error")
                .and_then(|e| e.get("code"))
                .and_then(|c| c.as_str())
                .unwrap_or("");
            if code.is_empty() {
                return msg.to_owned();
            }
            return format!("{msg} (code: {code})");
        }
    }
    // 回退：截断原始 body
    let truncated: String = body.chars().take(200).collect();
    if truncated.len() < body.len() {
        format!("{truncated}…")
    } else {
        truncated
    }
}

fn extract_question_from_args(arguments: &str) -> String {
    serde_json::from_str::<serde_json::Value>(arguments)
        .ok()
        .and_then(|v| v.get("question")?.as_str().map(|s| s.to_owned()))
        .unwrap_or_else(|| "请提供更多信息。".to_owned())
}

/// 构建带滑动窗口的 ChatMessage 列表。
///
/// 保证 tool_call/tool_result 配对不被截断。
fn build_chat_messages_with_window(
    system_prompt: &str,
    history: &[MessageRecord],
    max_messages: usize,
) -> Vec<ChatMessage> {
    let mut messages = Vec::new();

    if !system_prompt.trim().is_empty() {
        messages.push(ChatMessage::system(system_prompt.to_owned()));
    }

    if history.len() <= max_messages {
        for record in history {
            messages.push(record_to_chat_message(record));
        }
    } else {
        let split = find_safe_split_point(history, max_messages);
        let omitted = &history[..split];
        let included = &history[split..];

        if !omitted.is_empty() {
            let summary = summarize_omitted(omitted);
            messages.push(ChatMessage::system(summary));
        }

        for record in included {
            messages.push(record_to_chat_message(record));
        }
    }

    // 总 token 预算：约 12000 字符（~3000 tokens），为模型输出留空间
    // 逐条从尾部移除旧消息直到总量达标（保留第一条 system + 最新2条）
    const TOTAL_CHAR_BUDGET: usize = 12_000;
    let total_chars: usize = messages.iter().map(|m| m.content_chars()).sum();
    if total_chars > TOTAL_CHAR_BUDGET && messages.len() > 3 {
        let mut excess = total_chars - TOTAL_CHAR_BUDGET;
        let mut to_remove = Vec::new();
        // 从第2条消息开始向前遍历（保留 system prompt）
        for (i, msg) in messages.iter().enumerate().skip(1) {
            if excess == 0 {
                break;
            }
            // 保留最后2条消息
            if i >= messages.len() - 2 {
                break;
            }
            let chars = msg.content_chars();
            if chars > 0 {
                to_remove.push(i);
                excess = excess.saturating_sub(chars);
            }
        }
        // 从后向前移除以保持索引正确，不插入占位符（避免干扰LLM理解）
        for &idx in to_remove.iter().rev() {
            messages.remove(idx);
        }
        // 在摘要消息中补充说明
        if !messages.is_empty() && messages[0].role == "system" {
            // 已有系统摘要，不需要额外插入
        }
    }

    messages
}

/// 找到不破坏 tool_call/tool_result 配对的安全截断点。
fn find_safe_split_point(history: &[MessageRecord], max_messages: usize) -> usize {
    let target = history.len().saturating_sub(max_messages);
    let mut split = target;

    while split < history.len() {
        let msg = &history[split];
        if msg.role == MessageRole::Tool {
            if let Some(ref tcid) = msg.tool_call_id {
                let has_predecessor = history[..split].iter().any(|m| {
                    m.role == MessageRole::Assistant && m.tool_calls.iter().any(|tc| tc.id == *tcid)
                });
                if !has_predecessor {
                    split += 1;
                    continue;
                }
            }
        }
        break;
    }
    split
}

/// 为被截断的消息生成摘要。
fn summarize_omitted(messages: &[MessageRecord]) -> String {
    let tool_count = messages
        .iter()
        .filter(|m| m.role == MessageRole::Tool)
        .count();
    let last_user = messages.iter().rev().find(|m| m.role == MessageRole::User);
    let mut summary = format!(
        "[之前的对话摘要：共 {} 条消息，{} 次工具调用]",
        messages.len(),
        tool_count
    );
    if let Some(msg) = last_user {
        if let Some(content) = &msg.content {
            let preview: String = content.chars().take(100).collect();
            summary.push_str(&format!("\n学生最近的提问：\"{preview}\""));
        }
    }
    summary
}

/// 截断单条消息内容，避免超长工具返回撑爆上下文。
/// 限制单条内容不超过 `max_chars` 字符，并剥离 base64 图片数据。
fn truncate_message_content(content: &str, max_chars: usize) -> String {
    // 先剥离 base64 图片数据（保留标记）
    let cleaned = strip_base64_images(content);
    if cleaned.len() <= max_chars {
        return cleaned;
    }
    // 截断并添加标记
    let mut truncated: String = cleaned.chars().take(max_chars).collect();
    truncated.push_str("\n...[内容已截断，原长度超过上下文限制]");
    truncated
}

/// 将 base64 图片数据替换为占位符，大幅减少 token 消耗。
fn strip_base64_images(content: &str) -> String {
    // 匹配 data:image/xxx;base64,AAAA... 模式
    let mut result = String::with_capacity(content.len());
    let mut remaining = content;
    while let Some(start) = remaining.find("data:image/") {
        // 找到 base64 数据的起始
        if let Some(data_start) = remaining[start..].find("base64,") {
            let abs_data_start = start + data_start + 7; // "base64,".len() = 7
                                                         // 找到结束引号、逗号或换行
            let abs_end = remaining[abs_data_start..]
                .find(|c: char| c == '"' || c == '\'' || c == ',' || c == '\n' || c == '}')
                .map(|p| abs_data_start + p)
                .unwrap_or(remaining.len());
            let before = &remaining[..start];
            result.push_str(before);
            result.push_str("[base64-image-data]");
            remaining = &remaining[abs_end..];
        } else {
            // 没有 base64 标记，跳过
            result.push_str(&remaining[..start + 11]);
            remaining = &remaining[start + 11..];
        }
    }
    result.push_str(remaining);
    result
}

/// 将 MessageRecord 转换为 ChatMessage，带内容截断。
fn record_to_chat_message(record: &MessageRecord) -> ChatMessage {
    match record.role {
        MessageRole::System => {
            let raw = record.content.clone().unwrap_or_default();
            ChatMessage::system(truncate_message_content(&raw, 8000))
        }
        MessageRole::User => {
            let raw = record.content.clone().unwrap_or_default();
            // 检测多模态格式，但执行阶段不发送图片数据（LLM 可能不支持视觉），
            // 只保留文字部分（图片分析结果已注入文字中）。
            if let Some((text, _images)) = parse_multimodal_user_content(&raw) {
                ChatMessage::user(truncate_message_content(&text, 4000))
            } else {
                ChatMessage::user(truncate_message_content(&raw, 4000))
            }
        }
        MessageRole::Assistant => {
            let content = record
                .content
                .clone()
                .map(|c| truncate_message_content(&c, 4000));
            ChatMessage::assistant(content, record.tool_calls.clone())
        }
        MessageRole::Tool => {
            let raw = record.content.clone().unwrap_or_default();
            // 工具返回通常很长，限制到 2000 字符
            ChatMessage::tool(
                record.tool_call_id.clone().unwrap_or_default(),
                truncate_message_content(&raw, 2000),
            )
        }
    }
}

/// 尝试解析多模态用户消息格式。
/// 格式：`{"text":"用户文字","images":["data:image/...;base64,..."]}`
fn parse_multimodal_user_content(raw: &str) -> Option<(String, Vec<String>)> {
    let v: serde_json::Value = serde_json::from_str(raw).ok()?;
    let text = v.get("text")?.as_str()?.to_owned();
    let images = v
        .get("images")?
        .as_array()?
        .iter()
        .filter_map(|i| i.as_str().map(String::from))
        .collect::<Vec<_>>();
    if images.is_empty() {
        None
    } else {
        Some((text, images))
    }
}

pub(crate) fn emit_event(app: &AppHandle, conversation_id: &str, event: AgentEvent) {
    let kind = match &event {
        AgentEvent::MessageAppended { message, .. } => {
            format!("messageAppended(role={})", message.role.as_str())
        }
        AgentEvent::ToolInvocationUpdated { invocation, .. } => {
            format!("toolInvocationUpdated({})", invocation.tool_name)
        }
        AgentEvent::Done { .. } => "done".to_owned(),
        AgentEvent::Failed { .. } => "failed".to_owned(),
        AgentEvent::PlanCreated { .. } => "planCreated".to_owned(),
        AgentEvent::PlanUpdated { .. } => "planUpdated".to_owned(),
        AgentEvent::PlanStepChanged { .. } => "planStepChanged".to_owned(),
        AgentEvent::UserQuestionAsked { .. } => "userQuestionAsked".to_owned(),
        AgentEvent::MemoryRecalled { .. } => "memoryRecalled".to_owned(),
        AgentEvent::StreamChunk { chunk_index, .. } => format!("streamChunk(#{chunk_index})"),
        AgentEvent::StreamDone { total_chunks, .. } => format!("streamDone(total={total_chunks})"),
        AgentEvent::DirectorFallback { .. } => "directorFallback".to_owned(),
    };
    match app.emit("agent://event", event) {
        Ok(()) => eprintln!("[AgentEvent] emit ok kind={kind} conv={conversation_id}"),
        Err(error) => {
            eprintln!("[AgentEvent] emit FAILED kind={kind} conv={conversation_id} err={error}")
        }
    }
}

/// 将 CreativeRuntimeService 的执行结果格式化为对话消息文本。
fn format_runtime_result(
    result: &crate::application::creative_runtime_service::RuntimeResult,
) -> String {
    use crate::domain::execution::ExecutionStatus;

    let status_label = match result.status {
        ExecutionStatus::Completed => "全部完成",
        ExecutionStatus::Partial => "部分完成",
        ExecutionStatus::Failed => "执行失败",
        ExecutionStatus::Cancelled => "已取消",
    };

    let mut text = format!(
        "创作流水线执行{status_label}（耗时 {:.1}s）\n",
        result.duration_secs
    );

    if !result.shot_artifacts.is_empty() {
        text.push_str(&format!(
            "\n成功导入 {} 个素材资产。",
            result.shot_artifacts.len()
        ));
    }

    if let Some(path) = &result.composed_video_path {
        text.push_str(&format!("\n最终作品：{path}"));
    }

    if !result.errors.is_empty() {
        text.push_str(&format!("\n\n有 {} 个镜头遇到问题：", result.errors.len()));
        for (shot, reason) in &result.errors {
            text.push_str(&format!("\n- 镜头 {shot}：{reason}"));
        }
    }

    text
}

impl Reloadable for AgentService {
    fn reload(&self, database_path: &Path) -> Result<(), AppError> {
        use crate::adapters::providers::vision_adapter::ClaudeVisionAdapter;
        use crate::adapters::sqlite::credential_repository::SqliteCredentialRepository;
        use crate::adapters::sqlite::generation_attempt_repository::SqliteGenerationAttemptRepository;
        use crate::adapters::sqlite::generation_repository::SqliteGenerationRepository;
        use crate::adapters::sqlite::plan_repository::SqlitePlanRepository;
        use crate::adapters::sqlite::SqliteAgentRepository;

        let new_agent = SqliteAgentRepository::open(database_path)?;
        let new_credential = SqliteCredentialRepository::open(database_path)?;
        let new_generation = SqliteGenerationRepository::open(database_path)?;
        let new_credential_for_tools = SqliteCredentialRepository::open(database_path)?;
        let new_router_credential = SqliteCredentialRepository::open(database_path)?;
        let new_router_credential_service =
            crate::application::credential_service::CredentialService::new(
                new_router_credential,
                database_path.to_path_buf(),
            );
        let new_model_router = crate::application::model_router_service::ModelRouterService::new(
            new_router_credential_service,
            database_path.to_path_buf(),
        );
        let mut new_tool_executor = crate::adapters::agent::BuiltinToolExecutor::new(
            new_generation,
            new_credential_for_tools,
        )
        .with_model_router(new_model_router)
        .with_vision_adapter(ClaudeVisionAdapter::new())
        .with_canvas_context_service({
            let canvas_repo = Arc::new(std::sync::Mutex::new(
                crate::adapters::sqlite::canvas_repository::SqliteCanvasRepository::open(
                    database_path,
                )?,
            ));
            Arc::new(
                crate::application::canvas_context_service::CanvasContextService::new(canvas_repo),
            )
        })
        .with_apply_script_plan_service({
            let canvas_repo = Arc::new(std::sync::Mutex::new(
                crate::adapters::sqlite::canvas_repository::SqliteCanvasRepository::open(
                    database_path,
                )?,
            ));
            Arc::new(
                crate::application::apply_script_plan_service::ApplyScriptPlanService::new(
                    canvas_repo,
                ),
            )
        });

        if let (Some(provider_registry), Some(pipeline)) = (
            self.generation_provider_registry.as_ref(),
            self.generation_pipeline.as_ref(),
        ) {
            let attempt_repository = SqliteGenerationAttemptRepository::open(database_path)?;
            let submitter = GenerationSubmitService::new(
                attempt_repository,
                Arc::clone(provider_registry),
                Arc::clone(pipeline),
                database_path.to_path_buf(),
            )?;
            new_tool_executor = new_tool_executor.with_generation_submitter(submitter);
        }

        // 将 ProviderRegistry 传给工具执行器，用于自动选择生成 Provider
        if let Some(provider_registry) = self.generation_provider_registry.as_ref() {
            new_tool_executor =
                new_tool_executor.with_provider_registry(Arc::clone(provider_registry));
        }

        let new_plan = SqlitePlanRepository::open(database_path)?;

        let mut agent_repository = self
            .agent_repository
            .lock()
            .map_err(|_| AppError::StateUnavailable)?;
        *agent_repository = Box::new(new_agent);
        let mut credential_repository = self
            .credential_repository
            .lock()
            .map_err(|_| AppError::StateUnavailable)?;
        *credential_repository = Box::new(new_credential);
        let mut tool_executor = self
            .tool_executor
            .lock()
            .map_err(|_| AppError::StateUnavailable)?;
        *tool_executor = Box::new(new_tool_executor);
        let mut plan_repo = self
            .plan_repository
            .lock()
            .map_err(|_| AppError::StateUnavailable)?;
        *plan_repo = Box::new(new_plan);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::agent::{ChatMessage, MessageRole, PlanStepStatus, ToolCall};

    #[test]
    fn build_planning_prompt_includes_user_system() {
        let custom = Some("你是一个猫咪专家".to_owned());
        let prompt = build_planning_system_prompt_with_memory(&custom, None, None);
        assert!(prompt.contains("猫咪专家"));
        assert!(prompt.contains("计划"));
    }

    #[test]
    fn build_planning_prompt_includes_memory() {
        let memory = Some("- 学生喜欢日系动漫风格");
        let prompt = build_planning_system_prompt_with_memory(&None, memory, None);
        assert!(prompt.contains("[相关记忆]"));
        assert!(prompt.contains("日系动漫"));
    }

    #[test]
    fn build_execution_prompt_shows_plan() {
        let steps = vec![
            PlanStep {
                index: 1,
                description: "生成图片".to_owned(),
                status: PlanStepStatus::Completed,
                kind: Default::default(),
            },
            PlanStep {
                index: 2,
                description: "调整风格".to_owned(),
                status: PlanStepStatus::InProgress,
                kind: Default::default(),
            },
        ];
        let prompt = build_execution_system_prompt("画一只猫", &steps[1], &steps, None);
        assert!(prompt.contains("✓ 1. 生成图片"));
        assert!(prompt.contains("→ 2. 调整风格"));
        assert!(prompt.contains("第 2 步"));
        assert!(!prompt.contains("[相关记忆]"));
    }

    #[test]
    fn build_execution_prompt_starts_with_static_prefix() {
        // 前缀缓存契约：无论目标/步骤/记忆如何变化，prompt 必须以静态前缀开头。
        let steps = vec![PlanStep {
            index: 1,
            description: "任意步骤".to_owned(),
            status: PlanStepStatus::Pending,
            kind: Default::default(),
        }];
        let a = build_execution_system_prompt("目标A", &steps[0], &steps, None);
        let b = build_execution_system_prompt("目标B", &steps[0], &steps, Some("- 学生喜欢水彩风"));
        assert!(a.starts_with(EXECUTION_STATIC_PREFIX));
        assert!(b.starts_with(EXECUTION_STATIC_PREFIX));
    }

    #[test]
    fn build_execution_prompt_injects_memory_at_tail() {
        let steps = vec![PlanStep {
            index: 1,
            description: "生成图片".to_owned(),
            status: PlanStepStatus::InProgress,
            kind: Default::default(),
        }];
        let prompt = build_execution_system_prompt(
            "画一只猫",
            &steps[0],
            &steps,
            Some("- (已知事实) 学生喜欢日系动漫风格"),
        );
        assert!(prompt.contains("[相关记忆]"));
        assert!(prompt.contains("日系动漫"));
        // 记忆必须位于当前步骤之后（尾部），不破坏前面的稳定前缀。
        let step_pos = prompt.find("[当前步骤]").unwrap();
        let mem_pos = prompt.find("[相关记忆]").unwrap();
        assert!(mem_pos > step_pos);
    }

    #[test]
    fn extract_question_parses_json() {
        let args = r#"{"question":"你喜欢什么风格？"}"#;
        assert_eq!(extract_question_from_args(args), "你喜欢什么风格？");
    }

    #[test]
    fn extract_question_handles_invalid_json() {
        assert_eq!(extract_question_from_args("not json"), "请提供更多信息。");
    }

    #[test]
    fn extract_direct_answer_parses_marker() {
        assert_eq!(
            extract_direct_answer("[直接回答] 你好！很高兴见到你。"),
            Some("你好！很高兴见到你。".to_owned())
        );
        assert_eq!(
            extract_direct_answer("[直接回答]：你好！"),
            Some("你好！".to_owned())
        );
        assert_eq!(
            extract_direct_answer("  [直接回答]: 在的，有什么可以帮你？"),
            Some("在的，有什么可以帮你？".to_owned())
        );
    }

    #[test]
    fn extract_direct_answer_rejects_non_direct_content() {
        // 无标记 → 正常走计划解析
        assert_eq!(extract_direct_answer("1. 收集素材\n2. 生成图片"), None);
        // 标记后内容为空 → 视为无效
        assert_eq!(extract_direct_answer("[直接回答]"), None);
        assert_eq!(extract_direct_answer("[直接回答]：  "), None);
    }

    #[test]
    fn window_preserves_tool_pairs() {
        let history = vec![
            MessageRecord {
                id: "u1".to_owned(),
                conversation_id: "c".to_owned(),
                role: MessageRole::User,
                content: Some("画猫".to_owned()),
                tool_calls: vec![],
                tool_call_id: None,
                remote_model: None,
                finish_reason: None,
                parent_message_id: None,
                revision: 1,
                created_at: "t1".to_owned(),
                prompt_tokens: None,
                completion_tokens: None,
            },
            MessageRecord {
                id: "a1".to_owned(),
                conversation_id: "c".to_owned(),
                role: MessageRole::Assistant,
                content: None,
                tool_calls: vec![ToolCall::new(
                    "tc1".to_owned(),
                    "image_generation".to_owned(),
                    "{}".to_owned(),
                )],
                tool_call_id: None,
                remote_model: Some("grok".to_owned()),
                finish_reason: Some("tool_calls".to_owned()),
                parent_message_id: None,
                revision: 1,
                created_at: "t2".to_owned(),
                prompt_tokens: None,
                completion_tokens: None,
            },
            MessageRecord {
                id: "t1".to_owned(),
                conversation_id: "c".to_owned(),
                role: MessageRole::Tool,
                content: Some(r#"{"status":"ok"}"#.to_owned()),
                tool_calls: vec![],
                tool_call_id: Some("tc1".to_owned()),
                remote_model: None,
                finish_reason: None,
                parent_message_id: None,
                revision: 1,
                created_at: "t3".to_owned(),
                prompt_tokens: None,
                completion_tokens: None,
            },
        ];
        let messages = build_chat_messages_with_window("系统", &history, 2);
        // 滑动窗口为 2，但 tool 消息需要其 assistant 前驱，所以 3 条都保留
        assert_eq!(messages.len(), 4); // system + 3 history
    }

    #[test]
    fn summarize_omitted_counts_correctly() {
        let messages = vec![MessageRecord {
            id: "u1".to_owned(),
            conversation_id: "c".to_owned(),
            role: MessageRole::User,
            content: Some("你好".to_owned()),
            tool_calls: vec![],
            tool_call_id: None,
            remote_model: None,
            finish_reason: None,
            parent_message_id: None,
            revision: 1,
            created_at: "t".to_owned(),
            prompt_tokens: None,
            completion_tokens: None,
        }];
        let summary = summarize_omitted(&messages);
        assert!(summary.contains("1 条消息"));
    }
}
