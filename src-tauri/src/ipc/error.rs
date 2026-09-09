use serde::Serialize;

use crate::{
    application::error::AppError,
    ports::{
        agent_repository::AgentRepositoryError, asset_repository::AssetRepositoryError,
        backup_repository::BackupRepositoryError, credential_repository::CredentialRepositoryError,
        generation_repository::GenerationRepositoryError,
        resource_repository::ResourceRepositoryError,
    },
};

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IpcError {
    pub code: &'static str,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub field: Option<&'static str>,
}

impl IpcError {
    pub fn task_failed() -> Self {
        Self {
            code: "native_task_failed",
            message: "本地任务未能完成，请重试或重启应用。".to_owned(),
            field: None,
        }
    }
}

impl From<AppError> for IpcError {
    fn from(error: AppError) -> Self {
        match error {
            AppError::Validation(error) => Self {
                code: "validation_failed",
                message: error.user_message(),
                field: Some(error.field()),
            },
            AppError::AssetValidation(error) => Self {
                code: "validation_failed",
                message: error.user_message(),
                field: error.field(),
            },
            AppError::ResourceValidation(error) => Self {
                code: "validation_failed",
                message: error.user_message(),
                field: error.field(),
            },
            AppError::BackupValidation(error) => Self {
                code: "validation_failed",
                message: error.user_message(),
                field: error.field(),
            },
            AppError::GenerationValidation(error) => Self {
                code: "validation_failed",
                message: error.user_message(),
                field: error.field(),
            },
            AppError::AlreadyInitialized => Self {
                code: "workspace_already_initialized",
                message: "本地工作空间已经初始化。".to_owned(),
                field: None,
            },
            AppError::StateUnavailable => Self {
                code: "workspace_busy",
                message: "工作空间正在处理其他操作，请稍后重试。".to_owned(),
                field: None,
            },
            AppError::Persistence(error) => {
                eprintln!("workspace persistence error: {error}");
                Self {
                    code: "workspace_unavailable",
                    message: "本地工作空间暂时不可用，请重启应用后重试。".to_owned(),
                    field: None,
                }
            }
            AppError::AssetRepository(error) => match error {
                AssetRepositoryError::Staging(error) => {
                    eprintln!("asset staging error: {error}");
                    Self {
                        code: "asset_storage_unavailable",
                        message: "受管文件目录暂不可用，请检查磁盘空间或权限后重试。".to_owned(),
                        field: None,
                    }
                }
                AssetRepositoryError::Persistence(error) => {
                    eprintln!("asset persistence error: {error}");
                    Self {
                        code: "asset_data_unavailable",
                        message: "资产数据暂时不可用，请重试或重启应用。".to_owned(),
                        field: None,
                    }
                }
                AssetRepositoryError::Integrity(error) => {
                    eprintln!("asset integrity error: {error}");
                    Self {
                        code: "asset_integrity_unavailable",
                        message: "无法读取受管文件完成完整性校验，请检查文件权限后重试。"
                            .to_owned(),
                        field: None,
                    }
                }
            },
            AppError::ResourceRepository(error) => match error {
                ResourceRepositoryError::AssetNotFound(_) => Self {
                    code: "record_not_found",
                    message: "关联的资产不存在或已被移除。".to_owned(),
                    field: None,
                },
                ResourceRepositoryError::Duplicate { asset_id: _ } => Self {
                    code: "duplicate_value",
                    message: "同一资产在该上下文中的关联已存在。".to_owned(),
                    field: None,
                },
                ResourceRepositoryError::Persistence(error) => {
                    eprintln!("resource persistence error: {error}");
                    Self {
                        code: "resource_data_unavailable",
                        message: "资源关联数据暂时不可用，请重试或重启应用。".to_owned(),
                        field: None,
                    }
                }
            },
            AppError::BackupRepository(error) => match error {
                BackupRepositoryError::Persistence(error) => {
                    eprintln!("backup persistence error: {error}");
                    Self {
                        code: "backup_unavailable",
                        message: "备份归档暂不可用，请检查磁盘空间或权限后重试。".to_owned(),
                        field: None,
                    }
                }
                BackupRepositoryError::InvalidArchive(reason) => {
                    eprintln!("invalid backup archive: {reason}");
                    Self {
                        code: "backup_archive_invalid",
                        message: "备份归档损坏或格式不正确，无法继续。".to_owned(),
                        field: None,
                    }
                }
                BackupRepositoryError::IncompatibleSchema {
                    backup_version,
                    current_version,
                } => Self {
                    code: "backup_incompatible_schema",
                    message: format!(
                        "备份的 schema 版本 {backup_version} 高于当前应用支持的 {current_version}，无法恢复。"
                    ),
                    field: None,
                },
            },
            AppError::GenerationRepository(error) => match error {
                GenerationRepositoryError::TaskNotFound(_) => Self {
                    code: "record_not_found",
                    message: "生成任务不存在或已被移除。".to_owned(),
                    field: None,
                },
                GenerationRepositoryError::AssetNotFound(_) => Self {
                    code: "record_not_found",
                    message: "关联的资产不存在或已被移除。".to_owned(),
                    field: None,
                },
                GenerationRepositoryError::DuplicateResult {
                    task_id: _,
                    asset_id: _,
                } => Self {
                    code: "duplicate_value",
                    message: "该生成任务的结果已存在。".to_owned(),
                    field: None,
                },
                GenerationRepositoryError::Persistence(error) => {
                    eprintln!("generation persistence error: {error}");
                    Self {
                        code: "generation_data_unavailable",
                        message: "生成历史数据暂时不可用，请重试或重启应用。".to_owned(),
                        field: None,
                    }
                }
            },
            AppError::CredentialValidation(error) => Self {
                code: "validation_failed",
                message: error.user_message(),
                field: error.field(),
            },
            AppError::Provider(error) => Self {
                code: "provider_request_failed",
                message: error.user_message(),
                field: None,
            },
            AppError::CredentialRepository(error) => match error {
                CredentialRepositoryError::NotFound(_) => Self {
                    code: "record_not_found",
                    message: "凭据不存在或已被移除。".to_owned(),
                    field: None,
                },
                CredentialRepositoryError::Keychain(msg) => {
                    eprintln!("keychain error: {msg}");
                    Self {
                        code: "credential_keychain_unavailable",
                        message: "系统密钥存储不可用，请检查系统权限后重试。".to_owned(),
                        field: None,
                    }
                }
                CredentialRepositoryError::Persistence(error) => {
                    eprintln!("credential persistence error: {error}");
                    Self {
                        code: "credential_data_unavailable",
                        message: "凭据数据暂时不可用，请重试或重启应用。".to_owned(),
                        field: None,
                    }
                }
            },
            AppError::AgentValidation(error) => Self {
                code: "validation_failed",
                message: error.user_message(),
                field: error.field(),
            },
            AppError::AgentRepository(error) => match error {
                AgentRepositoryError::ConversationNotFound(_) => Self {
                    code: "record_not_found",
                    message: "会话不存在或已被移除。".to_owned(),
                    field: None,
                },
                AgentRepositoryError::MessageNotFound(_) => Self {
                    code: "record_not_found",
                    message: "消息不存在或已被移除。".to_owned(),
                    field: None,
                },
                AgentRepositoryError::InvocationNotFound(_) => Self {
                    code: "record_not_found",
                    message: "工具调用记录不存在或已被移除。".to_owned(),
                    field: None,
                },
                AgentRepositoryError::Validation(error) => Self {
                    code: "validation_failed",
                    message: error.user_message(),
                    field: error.field(),
                },
                AgentRepositoryError::Persistence(error) => {
                    eprintln!("agent persistence error: {error}");
                    Self {
                        code: "agent_data_unavailable",
                        message: "Agent 数据暂时不可用，请重试或重启应用。".to_owned(),
                        field: None,
                    }
                }
            },
            AppError::AgentLoopTooManyIterations(iterations) => Self {
                code: "agent_loop_too_many_iterations",
                message: format!("Agent 推理循环超过最大轮数（{iterations}），请缩小问题范围或重试。"),
                field: None,
            },
            AppError::AgentLlmError(msg) => {
                eprintln!("agent llm error: {msg}");
                Self {
                    code: "agent_llm_failed",
                    message: "Agent 调用大模型失败，请检查凭据与网络后重试。".to_owned(),
                    field: None,
                }
            }
            AppError::AgentToolFailed(msg) => {
                eprintln!("agent tool failed: {msg}");
                Self {
                    code: "agent_tool_failed",
                    message: "Agent 工具执行失败，请稍后重试。".to_owned(),
                    field: None,
                }
            }
            AppError::AgentUserQuestionPending => Self {
                code: "agent_user_question_pending",
                message: "Agent 正在等待你的回答。".to_owned(),
                field: None,
            },
            AppError::AgentGenerationSubmitFailed(msg) => {
                eprintln!("agent generation submit failed: {msg}");
                Self {
                    code: "agent_generation_submit_failed",
                    message: msg,
                    field: None,
                }
            }
            AppError::PlanRepository(error) => {
                eprintln!("plan repository error: {error}");
                Self {
                    code: "plan_unavailable",
                    message: "执行计划数据暂不可用，请重试。".to_owned(),
                    field: None,
                }
            }
            AppError::MangaValidation(error) => Self {
                code: "validation_failed",
                message: error.user_message(),
                field: None,
            },
            AppError::MangaRepository(error) => {
                eprintln!("manga repository error: {error}");
                Self {
                    code: "manga_data_unavailable",
                    message: "漫剧数据暂不可用，请重试。".to_owned(),
                    field: None,
                }
            }
            AppError::AttemptRepository(error) => {
                eprintln!("attempt repository error: {error}");
                Self {
                    code: "attempt_data_unavailable",
                    message: "生成任务数据暂不可用，请重试。".to_owned(),
                    field: None,
                }
            }
            AppError::MemoryServiceUnavailable => Self {
                code: "memory_service_unavailable",
                message: "长期记忆服务未启动。请在设置中配置并启用 EverOS。".to_owned(),
                field: None,
            },
            AppError::MemoryServiceError(msg) => {
                eprintln!("memory service error: {msg}");
                Self {
                    code: "memory_service_error",
                    message: "记忆服务出错，请稍后重试。".to_owned(),
                    field: None,
                }
            }
            AppError::PlatformRunNotFound(id) => Self {
                code: "run_not_found",
                message: format!("创意运行 {} 不存在。", id),
                field: None,
            },
            AppError::NotFound(msg) => Self {
                code: "record_not_found",
                message: msg,
                field: None,
            },
            AppError::ReviewRepository(error) => {
                eprintln!("review repository error: {error}");
                Self {
                    code: "review_data_unavailable",
                    message: "评价数据暂不可用，请重试。".to_owned(),
                    field: None,
                }
            }
            AppError::EditRepository(error) => {
                eprintln!("edit repository error: {error}");
                Self {
                    code: "edit_data_unavailable",
                    message: "编辑数据暂不可用，请重试。".to_owned(),
                    field: None,
                }
            }
            AppError::ReviewValidation(error) => Self {
                code: "validation_failed",
                message: error.user_message(),
                field: error.field(),
            },
            AppError::EditValidation(error) => Self {
                code: "validation_failed",
                message: error.user_message(),
                field: error.field(),
            },
            AppError::MemoryValidation(error) => Self {
                code: "validation_failed",
                message: error.user_message(),
                field: error.field(),
            },
            AppError::CreativeMemoryRepository(error) => {
                eprintln!("creative memory repository error: {error}");
                Self {
                    code: "creative_memory_unavailable",
                    message: "创意记忆数据暂不可用，请重试。".to_owned(),
                    field: None,
                }
            }
            AppError::CreativeStateRepository(error) => {
                eprintln!("creative state repository error: {error}");
                Self {
                    code: "creative_state_unavailable",
                    message: "创作状态数据暂不可用，请重试。".to_owned(),
                    field: None,
                }
            }
            AppError::Workflow(msg) => {
                eprintln!("workflow error: {msg}");
                Self {
                    code: "workflow_error",
                    message: msg,
                    field: None,
                }
            }
        }
    }
}
