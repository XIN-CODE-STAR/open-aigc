//! 执行引擎：从 AgentService 提取的执行阶段逻辑。
//!
//! 职责：
//! - 逐步执行计划中的每个步骤
//! - 单步工具循环（ReAct 循环）
//! - 单工具执行（创建 invocation → 执行 → 更新 → 写入 tool 消息）
//! - 流式输出（StreamChunk / StreamDone 事件）
//! - 构建系统提示词和滑动窗口消息

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use crate::{
    application::error::AppError,
    domain::{
        agent::{
            ChatMessage, ChatRequest, ConversationRecord, MessageDraft, MessageRecord, MessageRole,
            PlanRecord, PlanStep, PlanStepStatus, ToolCall, ToolInvocationDraft,
            ToolInvocationRecord, ToolInvocationStatus, EXECUTE_MAX_ITERATIONS,
        },
        sandbox::SandboxPolicy,
    },
    ports::{
        agent_llm::{AgentLlm, AgentLlmError},
        agent_repository::AgentRepository,
        agent_tool_executor::{AgentToolError, AgentToolExecutor, ToolContext},
    },
};

use super::agent_service::{AgentEvent, SendMessageResult};

/// 执行引擎：负责 Agent 对话的执行阶段（ReAct 工具循环）。
///
/// 从 AgentService.run_execution_phase() + execute_single_step() + execute_single_tool() 提取。
pub(crate) struct ExecutionEngine {
    agent_repository: Mutex<Box<dyn AgentRepository>>,
    tool_executor: Mutex<Box<dyn AgentToolExecutor>>,
    database_path: PathBuf,
    /// 用户选择的生成输出目录（可选），由前端通过 IPC 设置。
    output_directory: std::sync::Mutex<Option<PathBuf>>,
    /// 沙箱策略：限制工具可访问的路径、网络、命令。
    sandbox: Option<Arc<SandboxPolicy>>,
}

impl ExecutionEngine {
    pub fn new(
        agent_repository: impl AgentRepository + 'static,
        tool_executor: impl AgentToolExecutor + 'static,
        database_path: PathBuf,
    ) -> Self {
        Self {
            agent_repository: Mutex::new(Box::new(agent_repository)),
            tool_executor: Mutex::new(Box::new(tool_executor)),
            database_path,
            output_directory: std::sync::Mutex::new(None),
            sandbox: None,
        }
    }

    /// 注入沙箱策略。
    pub fn with_sandbox(mut self, sandbox: Arc<SandboxPolicy>) -> Self {
        self.sandbox = Some(sandbox);
        self
    }

    /// 设置生成输出目录。前端在用户选择目录后调用。
    pub fn set_output_directory(&self, path: Option<PathBuf>) {
        if let Ok(mut dir) = self.output_directory.lock() {
            *dir = path;
        }
    }

    /// 执行阶段：逐步执行计划中的每个步骤。
    pub fn run_execution_phase(
        &self,
        app: &tauri::AppHandle,
        conversation_id: &str,
        plan: &PlanRecord,
        conversation: &ConversationRecord,
        llm: &mut dyn AgentLlm,
        tools: &[crate::domain::agent::ToolDefinition],
        model_name: &str,
        memory_context: Option<&str>,
        planning_engine: &super::planning_engine::PlanningEngine,
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
            current_plan = planning_engine.update_plan_step(
                &plan.id,
                step.index,
                PlanStepStatus::InProgress,
            )?;
            super::agent_service::emit_event(
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
                    #[allow(unused_assignments)]
                    {
                        current_plan = planning_engine.update_plan_step(
                            &plan.id,
                            step.index,
                            PlanStepStatus::Completed,
                        )?;
                    }
                    super::agent_service::emit_event(
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
                    return Ok(SendMessageResult {
                        conversation_id: conversation_id.to_owned(),
                        final_message: last_assistant,
                        invocations: all_invocations,
                        finish_reason: "user_question".to_owned(),
                    });
                }
                Err(AppError::AgentGenerationSubmitFailed(reason)) => {
                    planning_engine.update_plan_step(
                        &plan.id,
                        step.index,
                        PlanStepStatus::Failed,
                    )?;
                    super::agent_service::emit_event(
                        app,
                        conversation_id,
                        AgentEvent::PlanStepChanged {
                            conversation_id: conversation_id.to_owned(),
                            plan_id: plan.id.clone(),
                            step_index: step.index,
                            status: PlanStepStatus::Failed,
                        },
                    );
                    eprintln!(
                        "[ExecutionEngine] step {} stopped: generation submit failed: {reason}",
                        step.index
                    );
                    super::agent_service::emit_event(
                        app,
                        conversation_id,
                        AgentEvent::Done {
                            conversation_id: conversation_id.to_owned(),
                            finish_reason: "generation_failed".to_owned(),
                        },
                    );
                    return Ok(SendMessageResult {
                        conversation_id: conversation_id.to_owned(),
                        final_message: last_assistant,
                        invocations: all_invocations,
                        finish_reason: "generation_failed".to_owned(),
                    });
                }
                Err(_e) => {
                    planning_engine.update_plan_step(
                        &plan.id,
                        step.index,
                        PlanStepStatus::Failed,
                    )?;
                    super::agent_service::emit_event(
                        app,
                        conversation_id,
                        AgentEvent::PlanStepChanged {
                            conversation_id: conversation_id.to_owned(),
                            plan_id: plan.id.clone(),
                            step_index: step.index,
                            status: PlanStepStatus::Failed,
                        },
                    );
                }
            }

            if total_iterations >= EXECUTE_MAX_ITERATIONS {
                break;
            }
        }

        // 发送完成事件。
        super::agent_service::emit_event(
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
        app: &tauri::AppHandle,
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
        let stream_chunk_index = std::cell::Cell::new(0u32);
        loop {
            *iterations += 1;
            if *iterations > EXECUTE_MAX_ITERATIONS {
                return Err(AppError::AgentLoopTooManyIterations(EXECUTE_MAX_ITERATIONS));
            }

            let history = self.with_agent_repository(|repo| {
                repo.list_messages(conversation_id).map_err(AppError::from)
            })?;
            let chat_messages = build_chat_messages_with_window(system_prompt, &history, 20);
            let request = ChatRequest::new(model_name.to_owned(), chat_messages, tools.to_vec());

            let response = match llm.chat_stream(&request, &|chunk| {
                let index = stream_chunk_index.get();
                stream_chunk_index.set(index.saturating_add(1));
                super::agent_service::emit_event(
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
                    super::agent_service::emit_event(
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
                    super::agent_service::emit_event(
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
            super::agent_service::emit_event(
                app,
                conversation_id,
                AgentEvent::MessageAppended {
                    conversation_id: conversation_id.to_owned(),
                    message: assistant_record.clone(),
                },
            );
            *last_assistant = Some(assistant_record);
            *last_finish_reason = response.finish_reason.clone();

            if response.tool_calls.is_empty() {
                if let Some(full) = &response.content {
                    super::agent_service::emit_event(
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

            for tool_call in &response.tool_calls {
                if tool_call.function.name == "ask_user_question" {
                    let question = extract_question_from_args(&tool_call.function.arguments);
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
                    super::agent_service::emit_event(
                        app,
                        conversation_id,
                        AgentEvent::ToolInvocationUpdated {
                            conversation_id: conversation_id.to_owned(),
                            invocation: invocation.clone(),
                        },
                    );
                    let safe_tool_call_id = if tool_call.id.trim().is_empty() {
                        uuid::Uuid::new_v4().to_string()
                    } else {
                        tool_call.id.clone()
                    };
                    super::agent_service::emit_event(
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

                let is_generation_tool = tool_call.function.name == "image_generation"
                    || tool_call.function.name == "video_generation";

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

                // 生成工具成功后直接结束，不再调用 LLM 总结（减少冗余回复）
                if is_generation_tool && invocation_record.status == ToolInvocationStatus::Succeeded
                {
                    all_invocations.push(invocation_record);
                    // 写入一条简洁的成功消息
                    let success_msg = format!(
                        "{}任务已提交，正在后台生成中。",
                        if tool_call.function.name == "image_generation" {
                            "图片"
                        } else {
                            "视频"
                        }
                    );
                    let assistant_record = self.with_agent_repository(|repo| {
                        repo.append_message(MessageDraft::assistant(
                            conversation_id.to_owned(),
                            Some(success_msg),
                            Vec::new(),
                            Some(response.model.clone()),
                            Some("stop".to_owned()),
                            None,
                            None,
                        )?)
                        .map_err(AppError::from)
                    })?;
                    super::agent_service::emit_event(
                        app,
                        conversation_id,
                        AgentEvent::MessageAppended {
                            conversation_id: conversation_id.to_owned(),
                            message: assistant_record,
                        },
                    );
                    super::agent_service::emit_event(
                        app,
                        conversation_id,
                        AgentEvent::StreamDone {
                            conversation_id: conversation_id.to_owned(),
                            full_content: String::new(),
                            total_chunks: 0,
                        },
                    );
                    return Ok(());
                }

                all_invocations.push(invocation_record.clone());

                // 生成工具提交失败：如实报告并立即终止本次执行。
                // 否则凭据失效时循环会继续推进计划的后续步骤，
                // 造成连续空转提交与自问自答式的失控自驱动。
                if is_generation_tool {
                    let reason = invocation_record
                        .result_json
                        .as_deref()
                        .and_then(|raw| serde_json::from_str::<serde_json::Value>(raw).ok())
                        .and_then(|value| {
                            value["message"]
                                .as_str()
                                .map(std::borrow::ToOwned::to_owned)
                        })
                        .or_else(|| invocation_record.error_message.clone())
                        .unwrap_or_else(|| "未知原因".to_owned());
                    let failure_msg = format!(
                        "{}任务提交失败，本次执行已停止。原因：{reason}",
                        if tool_call.function.name == "image_generation" {
                            "图片"
                        } else {
                            "视频"
                        }
                    );
                    let assistant_record = self.with_agent_repository(|repo| {
                        repo.append_message(MessageDraft::assistant(
                            conversation_id.to_owned(),
                            Some(failure_msg.clone()),
                            Vec::new(),
                            Some(response.model.clone()),
                            Some("stop".to_owned()),
                            None,
                            None,
                        )?)
                        .map_err(AppError::from)
                    })?;
                    super::agent_service::emit_event(
                        app,
                        conversation_id,
                        AgentEvent::MessageAppended {
                            conversation_id: conversation_id.to_owned(),
                            message: assistant_record,
                        },
                    );
                    super::agent_service::emit_event(
                        app,
                        conversation_id,
                        AgentEvent::StreamDone {
                            conversation_id: conversation_id.to_owned(),
                            full_content: String::new(),
                            total_chunks: 0,
                        },
                    );
                    return Err(AppError::AgentGenerationSubmitFailed(failure_msg));
                }
            }
        }
    }

    fn execute_single_tool(
        &self,
        app: &tauri::AppHandle,
        conversation_id: &str,
        message_id: &str,
        tool_call: &ToolCall,
        workspace_id: &str,
    ) -> Result<ToolInvocationRecord, AppError> {
        let invocation = self.with_agent_repository(|repo| {
            repo.create_invocation(ToolInvocationDraft::try_new(
                message_id.to_owned(),
                conversation_id.to_owned(),
                tool_call.function.name.clone(),
                tool_call.function.arguments.clone(),
            )?)
            .map_err(AppError::from)
        })?;
        super::agent_service::emit_event(
            app,
            conversation_id,
            AgentEvent::ToolInvocationUpdated {
                conversation_id: conversation_id.to_owned(),
                invocation: invocation.clone(),
            },
        );

        let invocation = self.update_invocation(
            &invocation.id,
            conversation_id,
            ToolInvocationStatus::Running,
            None,
            None,
            None,
        )?;
        super::agent_service::emit_event(
            app,
            conversation_id,
            AgentEvent::ToolInvocationUpdated {
                conversation_id: conversation_id.to_owned(),
                invocation: invocation.clone(),
            },
        );

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
        // 对话中最近一张用户上传的图片：存在时图片生成工具自动走图生图（i2i），
        // 让结果贴合原图的版式与内容，而不是仅凭文字描述重画。
        let latest_user_image = self
            .with_agent_repository(|repo| {
                repo.list_messages(conversation_id).map_err(AppError::from)
            })?
            .iter()
            .rev()
            .filter(|m| m.role == crate::domain::agent::MessageRole::User)
            .find_map(|m| latest_image_from_content(m.content.as_deref()?));

        let ctx = ToolContext {
            workspace_id: workspace_id.to_owned(),
            workspace_path,
            output_directory,
            conversation_id: conversation_id.to_owned(),
            sandbox: self.sandbox.clone(),
            latest_user_image,
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

        let updated_invocation = self.update_invocation(
            &invocation.id,
            conversation_id,
            status,
            result_json.clone(),
            error_message,
            generation_task_id,
        )?;
        super::agent_service::emit_event(
            app,
            conversation_id,
            AgentEvent::ToolInvocationUpdated {
                conversation_id: conversation_id.to_owned(),
                invocation: updated_invocation.clone(),
            },
        );

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
        super::agent_service::emit_event(
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
        let invocations = self.with_agent_repository(|repo| {
            repo.list_invocations(conversation_id)
                .map_err(AppError::from)
        })?;
        invocations
            .into_iter()
            .find(|i| i.id == invocation_id)
            .ok_or_else(|| {
                crate::ports::agent_repository::AgentRepositoryError::InvocationNotFound(
                    invocation_id.to_owned(),
                )
                .into()
            })
    }

    fn mark_conversation_error(
        &self,
        conversation_id: &str,
        _message: &str,
    ) -> Result<(), AppError> {
        self.with_agent_repository(|repo| {
            repo.update_conversation_status(
                conversation_id,
                crate::domain::agent::ConversationStatus::Error,
            )
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

    /// 加载工具定义列表（供 AgentRuntime 调用）。
    pub fn list_tools(&self) -> Vec<crate::domain::agent::ToolDefinition> {
        self.with_tool_executor(|executor| Ok(executor.list_tools()))
            .unwrap_or_default()
    }

    /// 查询消息列表（供 MemoryContextBuilder 调用）。
    pub fn list_messages(&self, conversation_id: &str) -> Result<Vec<MessageRecord>, AppError> {
        self.with_agent_repository(|repo| {
            repo.list_messages(conversation_id).map_err(AppError::from)
        })
    }
}

/// 执行阶段静态提示词前缀。
///
/// 提示词缓存关键：该文本在所有执行调用中保持完全一致，
/// 且位于 system 消息开头，供应商侧的隐式前缀缓存即可跨步骤命中。
const EXECUTION_STATIC_PREFIX: &str =
    "你是一位友好的 AIGC 创作助手，正在帮助学生完成创作任务。\n\n\
    执行规则：\n\
    - 专注完成当前步骤，不要提前执行后续步骤。\n\
    - 如果需要学生提供信息，使用 ask_user_question 工具提问。\n\
    - 需要生成、修改、提取、合成或编辑图片时，必须调用 image_generation 工具（对话中有用户上传的图片时自动走图生图模式）。\n\
    - 需要生成视频时，必须调用 video_generation 工具。\n\
    - 调用工具后根据返回结果继续推进。\n\
    - 不要替学生回答问题，也不要替学生确认（例如不要自己写「是的，请继续」）；需要学生决定时，必须调用 ask_user_question 工具并停止等待。\n\
    - 严格执行当前步骤，不要扩大范围：用户没有要求的额外镜头、变体、系列内容一律不要生成。\n\
    - 回复要简洁：调用工具时只需一句话说明你在做什么（如「正在为你生成图片」），不需要长篇解释。\n\
    - 不要重复已经说过的内容，不要复述工具返回的原始数据。\n\
    - 用中文回复。\n\n\
    可用工具：\n\
    - image_generation(prompt): 生成图片，传入详细的图片描述提示词\n\
    - video_generation(prompt): 生成视频，传入详细的视频描述提示词\n\
    - current_time(): 获取当前时间\n\
    - ask_user_question(question): 向用户提问";

/// 构建执行阶段系统提示词。
///
/// 结构：静态前缀（利于前缀缓存）+ 动态尾部（目标/计划/当前步骤/记忆）。
/// 从多模态用户消息 content（{"images":[...],"text":...}）中取最后一张图片。
fn latest_image_from_content(content: &str) -> Option<String> {
    let value = serde_json::from_str::<serde_json::Value>(content).ok()?;
    let images = value["images"].as_array()?;
    images
        .iter()
        .filter_map(|item| item.as_str())
        .next_back()
        .map(str::to_owned)
}

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

    if let Some(mem) = memory_context {
        if !mem.trim().is_empty() {
            prompt.push_str("\n\n[相关记忆]\n");
            prompt.push_str(mem);
            prompt.push_str("\n请在执行时参考以上记忆，贴合学生偏好。");
        }
    }

    prompt
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

    const TOTAL_CHAR_BUDGET: usize = 12_000;
    let total_chars: usize = messages.iter().map(|m| m.content_chars()).sum();
    if total_chars > TOTAL_CHAR_BUDGET && messages.len() > 3 {
        let mut excess = total_chars - TOTAL_CHAR_BUDGET;
        let mut to_remove = Vec::new();
        for (i, msg) in messages.iter().enumerate().skip(1) {
            if excess == 0 {
                break;
            }
            if i >= messages.len() - 2 {
                break;
            }
            let chars = msg.content_chars();
            if chars > 0 {
                to_remove.push(i);
                excess = excess.saturating_sub(chars);
            }
        }
        for &idx in to_remove.iter().rev() {
            messages.remove(idx);
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
fn truncate_message_content(content: &str, max_chars: usize) -> String {
    let cleaned = strip_base64_images(content);
    if cleaned.len() <= max_chars {
        return cleaned;
    }
    let mut truncated: String = cleaned.chars().take(max_chars).collect();
    truncated.push_str("\n...[内容已截断，原长度超过上下文限制]");
    truncated
}

/// 将 base64 图片数据替换为占位符，大幅减少 token 消耗。
fn strip_base64_images(content: &str) -> String {
    let mut result = String::with_capacity(content.len());
    let mut remaining = content;
    while let Some(start) = remaining.find("data:image/") {
        if let Some(data_start) = remaining[start..].find("base64,") {
            let abs_data_start = start + data_start + 7;
            let abs_end = remaining[abs_data_start..]
                .find(|c: char| c == '"' || c == '\'' || c == ',' || c == '\n' || c == '}')
                .map(|p| abs_data_start + p)
                .unwrap_or(remaining.len());
            let before = &remaining[..start];
            result.push_str(before);
            result.push_str("[base64-image-data]");
            remaining = &remaining[abs_end..];
        } else {
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
            ChatMessage::tool(
                record.tool_call_id.clone().unwrap_or_default(),
                truncate_message_content(&raw, 2000),
            )
        }
    }
}

/// 尝试解析多模态用户消息格式。
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

/// 从远端 API 的错误响应体中提取可读的错误描述。
fn extract_remote_error_detail(body: &str) -> String {
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
