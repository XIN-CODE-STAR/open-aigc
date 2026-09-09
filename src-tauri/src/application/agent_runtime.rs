//! Agent 运行时：组合 PlanningEngine + ExecutionEngine + MemoryContextBuilder。
//!
//! 职责：
//! - 作为 Agent 对话的顶层编排器
//! - 持久化用户消息
//! - 调用记忆召回 → 规划 → 执行（通用/创作快速路径）
//! - 响应缓存（直接回答类消息）
//! - 对话完成后存储记忆
//!
//! AgentService 变为薄 IPC 适配层，将 send_message() 委托给 AgentRuntime。

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use tauri::{AppHandle, Emitter};

use crate::{
    adapters::providers::grok_chat::GrokChatAdapter,
    application::{
        error::AppError, event_bus::EventBus, execution_engine::ExecutionEngine,
        memory_context_builder::MemoryContextBuilder, planning_engine::PlanningEngine,
        response_cache::ResponseCache,
    },
    domain::agent::MessageDraft,
    ports::{
        agent_repository::{AgentRepository, AgentRepositoryError},
        credential_repository::CredentialRepository,
    },
};

use super::agent_service::{AgentEvent, SendMessageResult};

/// Agent 运行时：顶层编排器。
///
/// 组合 PlanningEngine + ExecutionEngine + MemoryContextBuilder，
/// 替代原 AgentService 中的核心执行逻辑。
pub(crate) struct AgentRuntime {
    agent_repository: Mutex<Box<dyn AgentRepository>>,
    credential_repository: Mutex<Box<dyn CredentialRepository>>,
    planning_engine: PlanningEngine,
    execution_engine: ExecutionEngine,
    memory_context: MemoryContextBuilder,
    /// 轻量语义响应缓存：直接回答类消息命中后免 LLM 调用。
    response_cache: ResponseCache,
    /// 保留用于 Reloadable 和调试。
    #[allow(dead_code)]
    database_path: PathBuf,
    /// 统一事件总线：Agent 事件同时通过 EventBus 广播。
    event_bus: Option<Arc<EventBus>>,
    /// 语义分析管道：用于分析图片生成文字描述。
    semantic_pipeline:
        Option<Arc<crate::application::semantic::pipeline_service::SemanticPipelineService>>,
}

impl AgentRuntime {
    pub fn new(
        agent_repository: impl AgentRepository + 'static,
        credential_repository: impl CredentialRepository + 'static,
        planning_engine: PlanningEngine,
        execution_engine: ExecutionEngine,
        memory_context: MemoryContextBuilder,
        database_path: PathBuf,
    ) -> Self {
        Self {
            agent_repository: Mutex::new(Box::new(agent_repository)),
            credential_repository: Mutex::new(Box::new(credential_repository)),
            planning_engine,
            execution_engine,
            memory_context,
            response_cache: ResponseCache::new(),
            database_path,
            event_bus: None,
            semantic_pipeline: None,
        }
    }

    /// 注入统一事件总线。
    pub fn with_event_bus(mut self, bus: Arc<EventBus>) -> Self {
        self.event_bus = Some(bus);
        self
    }

    /// 注入语义分析管道（用于图片分析）。
    pub fn with_semantic_pipeline(
        mut self,
        pipeline: Arc<crate::application::semantic::pipeline_service::SemanticPipelineService>,
    ) -> Self {
        self.semantic_pipeline = Some(pipeline);
        self
    }

    /// 设置生成输出目录（透传给 ExecutionEngine）。
    pub fn set_output_directory(&self, path: Option<PathBuf>) {
        self.execution_engine.set_output_directory(path);
    }

    /// 发送用户消息并运行 Plan-and-Execute 循环。
    ///
    /// 流程：
    /// 1. 持久化用户消息
    /// 2. 记忆召回（一次召回，规划/执行共用）
    /// 3. 规划阶段：LLM 生成编号计划 → 持久化 → 推送 PlanCreated
    /// 4. 执行阶段：逐步执行计划，每步运行工具循环
    /// 5. 对话完成后存储记忆
    pub fn send_message(
        &self,
        app: &AppHandle,
        conversation_id: &str,
        user_content: String,
        image_data_urls: Vec<String>,
    ) -> Result<SendMessageResult, AppError> {
        eprintln!(
            "[AgentRuntime] send_message conv={} content_len={} attachments={}",
            conversation_id,
            user_content.len(),
            image_data_urls.len()
        );

        // 1. 持久化用户消息。
        let stored_content = if image_data_urls.is_empty() {
            user_content.clone()
        } else {
            // key 顺序：images 在 text 前，确保序列化后以 {"images": 开头
            // 不会误判为纯文本（BTreeMap 按字母序排列 key）
            serde_json::json!({
                "images": image_data_urls,
                "text": user_content,
            })
            .to_string()
        };
        let user_message = self.with_agent_repository(|repo| {
            repo.append_message(MessageDraft::user(
                conversation_id.to_owned(),
                stored_content,
            )?)
            .map_err(AppError::from)
        })?;
        emit_agent_event(
            app,
            conversation_id,
            AgentEvent::MessageAppended {
                conversation_id: conversation_id.to_owned(),
                message: user_message.clone(),
            },
        );

        // 2. 图片分析：如果有图片附件，先用语义管道分析图片生成文字描述。
        let image_analysis_context = if !image_data_urls.is_empty() {
            if let Some(pipeline) = &self.semantic_pipeline {
                let mut analyses = Vec::new();
                for (i, image_url) in image_data_urls.iter().enumerate() {
                    let asset_id = format!("{}_image_{}", conversation_id, i);
                    match pipeline.analyze_asset(&asset_id, image_url, None) {
                        Ok(result) => {
                            let profile = &result.profile;
                            let mut desc = Vec::new();
                            if let Some(caption) = &profile.caption {
                                desc.push(format!("图片描述：{}", caption));
                            }
                            // OCR 是"改文字/去水印/去二维码"类任务的关键信息，
                            // 必须注入；description_short 是 caption 首句，跳过避免重复。
                            if let Some(ocr) = &profile.ocr_text {
                                desc.push(format!("图内文字（OCR）：{}", ocr));
                            }
                            if !profile.tags.is_empty() {
                                let tags: Vec<String> =
                                    profile.tags.iter().map(|t| t.name.clone()).collect();
                                desc.push(format!("标签：{}", tags.join(", ")));
                            }
                            if !profile.entities.is_empty() {
                                let entities: Vec<String> =
                                    profile.entities.iter().map(|e| e.name.clone()).collect();
                                desc.push(format!("实体：{}", entities.join(", ")));
                            }
                            if !desc.is_empty() {
                                analyses.push(format!(
                                    "[图片{}分析结果]\n{}",
                                    i + 1,
                                    desc.join("\n")
                                ));
                            }
                            eprintln!(
                                "[AgentRuntime] image {} analyzed: {:?}",
                                i + 1,
                                profile.caption
                            );
                        }
                        Err(e) => {
                            eprintln!("[AgentRuntime] image {} analysis failed: {}", i + 1, e);
                        }
                    }
                }
                if analyses.is_empty() {
                    None
                } else {
                    Some(analyses.join("\n\n"))
                }
            } else {
                eprintln!(
                    "[AgentRuntime] semantic_pipeline not available, skipping image analysis"
                );
                None
            }
        } else {
            None
        };

        // 如果有图片分析结果，将其注入到用户消息中。
        let enhanced_user_content = if let Some(context) = &image_analysis_context {
            format!(
                "{}\n\n[图片分析上下文]\n{}\n\n请基于以上图片分析结果来理解用户的图片内容。",
                user_content, context
            )
        } else {
            user_content.clone()
        };

        // 3. 加载会话 + 凭据。
        let conversation = self.with_agent_repository(|repo| {
            repo.get_conversation(conversation_id)?
                .ok_or_else(|| {
                    AgentRepositoryError::ConversationNotFound(conversation_id.to_owned())
                })
                .map_err(AppError::from)
        })?;

        // 语义响应缓存：直接回答类消息命中后跳过 LLM。
        if let Some(cached) = self
            .response_cache
            .get(&user_content, conversation.system_prompt.as_deref())
        {
            eprintln!(
                "[AgentRuntime] response cache hit ({} chars), skipping LLM",
                cached.len()
            );
            let assistant_message = self.with_agent_repository(|repo| {
                repo.append_message(MessageDraft::assistant(
                    conversation_id.to_owned(),
                    Some(cached),
                    Vec::new(),
                    None,
                    Some("direct_answer_cache".to_owned()),
                    None,
                    None,
                )?)
                .map_err(AppError::from)
            })?;
            emit_agent_event(
                app,
                conversation_id,
                AgentEvent::MessageAppended {
                    conversation_id: conversation_id.to_owned(),
                    message: assistant_message.clone(),
                },
            );
            emit_agent_event(
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

        // 4. 记忆召回只做一次，结果同时注入规划与执行阶段。
        let (memory_context, memory_entries) = self
            .memory_context
            .recall_memory_with_entries(&conversation.workspace_id, &enhanced_user_content);

        // 4.5 画布工作记忆（RAG）：以当前请求检索最相关的画布节点并注入，
        // 让 agent 感知用户在无限画布上放置的图片、便签与结论。
        let memory_context = {
            let canvas_memory = crate::application::canvas_memory_rag::load_working_memory_context(
                &self.database_path,
                conversation_id,
                Some(&enhanced_user_content),
                6,
            );
            match (memory_context, canvas_memory) {
                (Some(memory), Some(canvas)) => Some(format!("{memory}\n\n{canvas}")),
                (Some(memory), None) => Some(memory),
                (None, Some(canvas)) => Some(canvas),
                (None, None) => None,
            }
        };
        if !memory_entries.is_empty() {
            emit_agent_event(
                app,
                conversation_id,
                AgentEvent::MemoryRecalled {
                    conversation_id: conversation_id.to_owned(),
                    memories: memory_entries,
                },
            );
        }

        // 如果图片已被语义管道分析并转为文字描述，则不再传原始图片数据给LLM（避免不支持视觉的模型报错）。
        let effective_image_urls: Vec<String> = if image_analysis_context.is_some() {
            Vec::new()
        } else {
            image_data_urls.clone()
        };

        // 使用增强后的用户内容（包含图片分析结果）。
        let outcome = self.planning_engine.run_planning_phase(
            app,
            conversation_id,
            &enhanced_user_content,
            &user_content,
            !image_data_urls.is_empty(),
            &conversation,
            &mut llm,
            &credential.model_name,
            memory_context.clone(),
            &effective_image_urls,
        )?;

        // 简单对话快速路径。
        let (plan, _creative_plan) = match outcome {
            super::planning_engine::PlanningOutcome::DirectAnswer { content } => {
                eprintln!(
                    "[AgentRuntime] direct answer fast path: {} chars",
                    content.len()
                );
                self.response_cache.put(
                    &enhanced_user_content,
                    conversation.system_prompt.as_deref(),
                    &content,
                );
                let assistant_message = self.with_agent_repository(|repo| {
                    repo.append_message(MessageDraft::assistant(
                        conversation_id.to_owned(),
                        Some(content),
                        Vec::new(),
                        None,
                        Some("direct_answer".to_owned()),
                        None,
                        None,
                    )?)
                    .map_err(AppError::from)
                })?;
                emit_agent_event(
                    app,
                    conversation_id,
                    AgentEvent::MessageAppended {
                        conversation_id: conversation_id.to_owned(),
                        message: assistant_message.clone(),
                    },
                );
                emit_agent_event(
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
            super::planning_engine::PlanningOutcome::Planned {
                plan,
                creative_plan,
            } => (plan, creative_plan),
        };

        // ── 创作快速路径：CreativePlan → CreativeRuntimeService ──
        // NOTE: creative_runtime 快速路径暂时保留在 AgentService 中，
        // 因为它依赖 AgentService 上的 creative_runtime 字段。
        // 后续可将 creative_runtime 注入到 AgentRuntime 中。

        // ── 通用执行阶段 ──
        let tools = self.execution_engine.list_tools();
        eprintln!(
            "[AgentRuntime] starting execution phase, plan steps={}",
            plan.steps.len()
        );
        let result = self.execution_engine.run_execution_phase(
            app,
            conversation_id,
            &plan,
            &conversation,
            &mut llm,
            &tools,
            &credential.model_name,
            memory_context.as_deref(),
            &self.planning_engine,
        );
        match &result {
            Ok(r) => eprintln!(
                "[AgentRuntime] execution phase done, finish_reason={}",
                r.finish_reason
            ),
            Err(e) => eprintln!("[AgentRuntime] execution phase failed: {e}"),
        }

        // 对话完成后存储到长期记忆。
        if let Ok(history) = self.execution_engine.list_messages(conversation_id) {
            self.memory_context
                .store_memory(&conversation.workspace_id, conversation_id, &history);
        }

        result
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
}

/// 发射 Agent 事件到 Tauri 前端 + EventBus。
fn emit_agent_event(app: &AppHandle, conversation_id: &str, event: AgentEvent) {
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
