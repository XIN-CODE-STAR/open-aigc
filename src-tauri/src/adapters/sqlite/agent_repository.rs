use std::path::Path;

use rusqlite::{params, OptionalExtension, TransactionBehavior};
use uuid::Uuid;

use crate::{
    domain::agent::{
        ConversationDraft, ConversationRecord, ConversationStatus, ExecutionMode, MessageDraft,
        MessageRecord, MessageRole, ToolCall, ToolInvocationDraft, ToolInvocationRecord,
        ToolInvocationStatus,
    },
    ports::{
        agent_repository::{AgentRepository, AgentRepositoryError},
        persistence::PersistenceError,
    },
};

use super::now_rfc3339;

pub struct SqliteAgentRepository {
    connection: rusqlite::Connection,
}

impl SqliteAgentRepository {
    pub fn open(database_path: impl AsRef<Path>) -> Result<Self, PersistenceError> {
        use crate::adapters::sqlite::database::open_database;
        let connection = open_database(database_path.as_ref())?;
        Ok(Self { connection })
    }
}

impl AgentRepository for SqliteAgentRepository {
    fn create_conversation(
        &mut self,
        draft: ConversationDraft,
    ) -> Result<ConversationRecord, AgentRepositoryError> {
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|e| PersistenceError::new("begin agent conversation transaction", e))?;
        let id = Uuid::new_v4().to_string();
        let now = now_rfc3339()?;

        transaction
            .execute(
                r#"
                INSERT INTO agent_conversations
                  (id, workspace_id, title, credential_id, system_prompt,
                   status, revision, created_at, updated_at)
                VALUES (?1, ?2, ?3, ?4, ?5, 'active', 1, ?6, ?6)
                "#,
                params![
                    id,
                    draft.workspace_id,
                    draft.title,
                    draft.credential_id,
                    draft.system_prompt,
                    now
                ],
            )
            .map_err(|e| PersistenceError::new("insert agent conversation", e))?;

        transaction
            .commit()
            .map_err(|e| PersistenceError::new("commit agent conversation", e))?;

        Ok(ConversationRecord {
            id,
            workspace_id: draft.workspace_id,
            title: draft.title,
            credential_id: draft.credential_id,
            system_prompt: draft.system_prompt,
            status: ConversationStatus::Active,
            execution_mode: ExecutionMode::PlanAndExecute,
            loop_state_json: None,
            revision: 1,
            created_at: now.clone(),
            updated_at: now,
        })
    }

    fn get_conversation(
        &mut self,
        conversation_id: &str,
    ) -> Result<Option<ConversationRecord>, AgentRepositoryError> {
        let record = self
            .connection
            .query_row(
                r#"
                SELECT id, workspace_id, title, credential_id, system_prompt,
                       status, revision, created_at, updated_at,
                       execution_mode, loop_state_json
                FROM agent_conversations WHERE id = ?1
                "#,
                params![conversation_id],
                map_conversation,
            )
            .optional()
            .map_err(|e| PersistenceError::new("read agent conversation", e))?;
        Ok(record)
    }

    fn list_conversations(
        &mut self,
        workspace_id: &str,
    ) -> Result<Vec<ConversationRecord>, AgentRepositoryError> {
        let mut statement = self
            .connection
            .prepare(
                r#"
                SELECT id, workspace_id, title, credential_id, system_prompt,
                       status, revision, created_at, updated_at,
                       execution_mode, loop_state_json
                FROM agent_conversations
                WHERE workspace_id = ?1
                ORDER BY updated_at DESC
                "#,
            )
            .map_err(|e| PersistenceError::new("prepare agent conversation list", e))?;
        let rows = statement
            .query_map(params![workspace_id], map_conversation)
            .map_err(|e| PersistenceError::new("query agent conversation list", e))?;
        let mut records = Vec::new();
        for row in rows {
            records.push(row.map_err(|e| PersistenceError::new("read conversation row", e))?);
        }
        Ok(records)
    }

    fn update_conversation_status(
        &mut self,
        conversation_id: &str,
        status: ConversationStatus,
    ) -> Result<ConversationRecord, AgentRepositoryError> {
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|e| PersistenceError::new("begin conversation status transaction", e))?;
        let now = now_rfc3339()?;
        let affected = transaction
            .execute(
                "UPDATE agent_conversations SET status = ?1, updated_at = ?2 WHERE id = ?3",
                params![status.as_str(), now, conversation_id],
            )
            .map_err(|e| PersistenceError::new("update conversation status", e))?;
        if affected == 0 {
            return Err(AgentRepositoryError::ConversationNotFound(
                conversation_id.to_owned(),
            ));
        }
        let record = transaction
            .query_row(
                r#"
                SELECT id, workspace_id, title, credential_id, system_prompt,
                       status, revision, created_at, updated_at,
                       execution_mode, loop_state_json
                FROM agent_conversations WHERE id = ?1
                "#,
                params![conversation_id],
                map_conversation,
            )
            .map_err(|e| PersistenceError::new("read updated conversation", e))?;
        transaction
            .commit()
            .map_err(|e| PersistenceError::new("commit conversation status", e))?;
        Ok(record)
    }

    fn rename_conversation(
        &mut self,
        conversation_id: &str,
        title: &str,
    ) -> Result<ConversationRecord, AgentRepositoryError> {
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|e| PersistenceError::new("begin conversation rename transaction", e))?;
        let now = now_rfc3339()?;
        let affected = transaction
            .execute(
                "UPDATE agent_conversations SET title = ?1, updated_at = ?2 WHERE id = ?3",
                params![title, now, conversation_id],
            )
            .map_err(|e| PersistenceError::new("rename conversation", e))?;
        if affected == 0 {
            return Err(AgentRepositoryError::ConversationNotFound(
                conversation_id.to_owned(),
            ));
        }
        let record = transaction
            .query_row(
                r#"
                SELECT id, workspace_id, title, credential_id, system_prompt,
                       status, revision, created_at, updated_at,
                       execution_mode, loop_state_json
                FROM agent_conversations WHERE id = ?1
                "#,
                params![conversation_id],
                map_conversation,
            )
            .map_err(|e| PersistenceError::new("read renamed conversation", e))?;
        transaction
            .commit()
            .map_err(|e| PersistenceError::new("commit conversation rename", e))?;
        Ok(record)
    }

    fn delete_conversation(&mut self, conversation_id: &str) -> Result<(), AgentRepositoryError> {
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|e| PersistenceError::new("begin conversation delete transaction", e))?;
        // 顺序：先删工具调用、消息，再删会话。
        transaction
            .execute(
                "DELETE FROM agent_tool_invocations WHERE conversation_id = ?1",
                params![conversation_id],
            )
            .map_err(|e| PersistenceError::new("delete conversation invocations", e))?;
        transaction
            .execute(
                "DELETE FROM agent_messages WHERE conversation_id = ?1",
                params![conversation_id],
            )
            .map_err(|e| PersistenceError::new("delete conversation messages", e))?;
        let affected = transaction
            .execute(
                "DELETE FROM agent_conversations WHERE id = ?1",
                params![conversation_id],
            )
            .map_err(|e| PersistenceError::new("delete conversation", e))?;
        if affected == 0 {
            return Err(AgentRepositoryError::ConversationNotFound(
                conversation_id.to_owned(),
            ));
        }
        transaction
            .commit()
            .map_err(|e| PersistenceError::new("commit conversation delete", e))?;
        Ok(())
    }

    fn append_message(
        &mut self,
        draft: MessageDraft,
    ) -> Result<MessageRecord, AgentRepositoryError> {
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|e| PersistenceError::new("begin agent message transaction", e))?;
        let id = Uuid::new_v4().to_string();
        let now = now_rfc3339()?;
        let tool_calls_json = if draft.tool_calls.is_empty() {
            None
        } else {
            Some(
                serde_json::to_string(&draft.tool_calls)
                    .map_err(|e| PersistenceError::new("serialize message tool_calls", e))?,
            )
        };

        transaction
            .execute(
                r#"
                INSERT INTO agent_messages
                  (id, conversation_id, role, content, tool_calls_json, tool_call_id,
                   remote_model, finish_reason, parent_message_id, revision,
                   created_at, prompt_tokens, completion_tokens)
                VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, 1, ?10, ?11, ?12)
                "#,
                params![
                    id,
                    draft.conversation_id,
                    draft.role.as_str(),
                    draft.content,
                    tool_calls_json,
                    draft.tool_call_id,
                    draft.remote_model,
                    draft.finish_reason,
                    draft.parent_message_id,
                    now,
                    draft.prompt_tokens,
                    draft.completion_tokens
                ],
            )
            .map_err(|e| PersistenceError::new("insert agent message", e))?;

        // 更新会话的 updated_at 时间戳，便于 list_conversations 排序。
        transaction
            .execute(
                "UPDATE agent_conversations SET updated_at = ?1 WHERE id = ?2",
                params![now, draft.conversation_id],
            )
            .map_err(|e| PersistenceError::new("touch conversation", e))?;

        transaction
            .commit()
            .map_err(|e| PersistenceError::new("commit agent message", e))?;

        Ok(MessageRecord {
            id,
            conversation_id: draft.conversation_id,
            role: draft.role,
            content: draft.content,
            tool_calls: draft.tool_calls,
            tool_call_id: draft.tool_call_id,
            remote_model: draft.remote_model,
            finish_reason: draft.finish_reason,
            parent_message_id: draft.parent_message_id,
            revision: 1,
            created_at: now,
            prompt_tokens: draft.prompt_tokens,
            completion_tokens: draft.completion_tokens,
        })
    }

    fn list_messages(
        &mut self,
        conversation_id: &str,
    ) -> Result<Vec<MessageRecord>, AgentRepositoryError> {
        let mut statement = self
            .connection
            .prepare(
                r#"
                SELECT id, conversation_id, role, content, tool_calls_json, tool_call_id,
                       remote_model, finish_reason, parent_message_id, revision,
                       created_at, prompt_tokens, completion_tokens
                FROM agent_messages
                WHERE conversation_id = ?1
                ORDER BY created_at ASC
                "#,
            )
            .map_err(|e| PersistenceError::new("prepare agent message list", e))?;
        let rows = statement
            .query_map(params![conversation_id], map_message)
            .map_err(|e| PersistenceError::new("query agent message list", e))?;
        let mut records = Vec::new();
        for row in rows {
            records.push(row.map_err(|e| PersistenceError::new("read message row", e))?);
        }
        Ok(records)
    }

    fn create_invocation(
        &mut self,
        draft: ToolInvocationDraft,
    ) -> Result<ToolInvocationRecord, AgentRepositoryError> {
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|e| PersistenceError::new("begin invocation transaction", e))?;
        let id = Uuid::new_v4().to_string();
        let now = now_rfc3339()?;

        transaction
            .execute(
                r#"
                INSERT INTO agent_tool_invocations
                  (id, message_id, conversation_id, tool_name, arguments_json,
                   result_json, status, error_message, generation_task_id,
                   started_at, completed_at, created_at)
                VALUES (?1, ?2, ?3, ?4, ?5, NULL, 'pending', NULL, NULL,
                        NULL, NULL, ?6)
                "#,
                params![
                    id,
                    draft.message_id,
                    draft.conversation_id,
                    draft.tool_name,
                    draft.arguments_json,
                    now
                ],
            )
            .map_err(|e| PersistenceError::new("insert agent invocation", e))?;

        transaction
            .commit()
            .map_err(|e| PersistenceError::new("commit agent invocation", e))?;

        Ok(ToolInvocationRecord {
            id,
            message_id: draft.message_id,
            conversation_id: draft.conversation_id,
            tool_name: draft.tool_name,
            arguments_json: draft.arguments_json,
            result_json: None,
            status: ToolInvocationStatus::Pending,
            error_message: None,
            generation_task_id: None,
            started_at: None,
            completed_at: None,
            created_at: now,
        })
    }

    fn update_invocation_status(
        &mut self,
        invocation_id: &str,
        status: ToolInvocationStatus,
        result_json: Option<String>,
        error_message: Option<String>,
        generation_task_id: Option<String>,
    ) -> Result<ToolInvocationRecord, AgentRepositoryError> {
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|e| PersistenceError::new("begin invocation update transaction", e))?;
        let now = now_rfc3339()?;
        let started_at: Option<String> = if status == ToolInvocationStatus::Running {
            Some(now.clone())
        } else {
            None
        };
        let completed_at: Option<String> = if matches!(
            status,
            ToolInvocationStatus::Succeeded
                | ToolInvocationStatus::Failed
                | ToolInvocationStatus::Skipped
        ) {
            Some(now.clone())
        } else {
            None
        };

        let affected = transaction
            .execute(
                r#"
                UPDATE agent_tool_invocations
                SET status = ?1,
                    result_json = COALESCE(?2, result_json),
                    error_message = ?3,
                    generation_task_id = COALESCE(?4, generation_task_id),
                    started_at = COALESCE(started_at, ?5),
                    completed_at = ?6
                WHERE id = ?7
                "#,
                params![
                    status.as_str(),
                    result_json,
                    error_message,
                    generation_task_id,
                    started_at,
                    completed_at,
                    invocation_id
                ],
            )
            .map_err(|e| PersistenceError::new("update invocation status", e))?;
        if affected == 0 {
            return Err(AgentRepositoryError::InvocationNotFound(
                invocation_id.to_owned(),
            ));
        }

        let record = transaction
            .query_row(
                r#"
                SELECT id, message_id, conversation_id, tool_name, arguments_json,
                       result_json, status, error_message, generation_task_id,
                       started_at, completed_at, created_at
                FROM agent_tool_invocations WHERE id = ?1
                "#,
                params![invocation_id],
                map_invocation,
            )
            .map_err(|e| PersistenceError::new("read updated invocation", e))?;
        transaction
            .commit()
            .map_err(|e| PersistenceError::new("commit invocation update", e))?;
        Ok(record)
    }

    fn list_invocations(
        &mut self,
        conversation_id: &str,
    ) -> Result<Vec<ToolInvocationRecord>, AgentRepositoryError> {
        let mut statement = self
            .connection
            .prepare(
                r#"
                SELECT id, message_id, conversation_id, tool_name, arguments_json,
                       result_json, status, error_message, generation_task_id,
                       started_at, completed_at, created_at
                FROM agent_tool_invocations
                WHERE conversation_id = ?1
                ORDER BY created_at ASC
                "#,
            )
            .map_err(|e| PersistenceError::new("prepare invocation list", e))?;
        let rows = statement
            .query_map(params![conversation_id], map_invocation)
            .map_err(|e| PersistenceError::new("query invocation list", e))?;
        let mut records = Vec::new();
        for row in rows {
            records.push(row.map_err(|e| PersistenceError::new("read invocation row", e))?);
        }
        Ok(records)
    }
}

fn map_conversation(row: &rusqlite::Row<'_>) -> rusqlite::Result<ConversationRecord> {
    let status_str: String = row.get(5)?;
    let status = ConversationStatus::parse(&status_str).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(5, rusqlite::types::Type::Text, Box::new(e))
    })?;
    let exec_mode_str: Option<String> = row.get(9)?;
    let execution_mode = exec_mode_str
        .as_deref()
        .and_then(|s| ExecutionMode::parse(s).ok())
        .unwrap_or(ExecutionMode::PlanAndExecute);
    Ok(ConversationRecord {
        id: row.get(0)?,
        workspace_id: row.get(1)?,
        title: row.get(2)?,
        credential_id: row.get(3)?,
        system_prompt: row.get(4)?,
        status,
        execution_mode,
        loop_state_json: row.get(10)?,
        revision: row.get(6)?,
        created_at: row.get(7)?,
        updated_at: row.get(8)?,
    })
}

fn map_message(row: &rusqlite::Row<'_>) -> rusqlite::Result<MessageRecord> {
    let role_str: String = row.get(2)?;
    let role = MessageRole::parse(&role_str).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(2, rusqlite::types::Type::Text, Box::new(e))
    })?;
    let tool_calls_json: Option<String> = row.get(4)?;
    let tool_calls: Vec<ToolCall> = match tool_calls_json {
        Some(json) => serde_json::from_str(&json).map_err(|e| {
            rusqlite::Error::FromSqlConversionFailure(4, rusqlite::types::Type::Text, Box::new(e))
        })?,
        None => Vec::new(),
    };
    Ok(MessageRecord {
        id: row.get(0)?,
        conversation_id: row.get(1)?,
        role,
        content: row.get(3)?,
        tool_calls,
        tool_call_id: row.get(5)?,
        remote_model: row.get(6)?,
        finish_reason: row.get(7)?,
        parent_message_id: row.get(8)?,
        revision: row.get(9)?,
        created_at: row.get(10)?,
        prompt_tokens: row.get(11)?,
        completion_tokens: row.get(12)?,
    })
}

fn map_invocation(row: &rusqlite::Row<'_>) -> rusqlite::Result<ToolInvocationRecord> {
    let status_str: String = row.get(6)?;
    let status = ToolInvocationStatus::parse(&status_str).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(6, rusqlite::types::Type::Text, Box::new(e))
    })?;
    Ok(ToolInvocationRecord {
        id: row.get(0)?,
        message_id: row.get(1)?,
        conversation_id: row.get(2)?,
        tool_name: row.get(3)?,
        arguments_json: row.get(4)?,
        result_json: row.get(5)?,
        status,
        error_message: row.get(7)?,
        generation_task_id: row.get(8)?,
        started_at: row.get(9)?,
        completed_at: row.get(10)?,
        created_at: row.get(11)?,
    })
}

#[cfg(test)]
mod tests {
    use tempfile::tempdir;

    use super::*;
    use crate::adapters::sqlite::credential_repository::SqliteCredentialRepository;
    use crate::adapters::sqlite::workspace_repository::SqliteWorkspaceRepository;
    use crate::domain::credentials::CredentialDraft;
    use crate::domain::workspace::NewWorkspace;
    use crate::ports::credential_repository::CredentialRepository;
    use crate::ports::workspace_repository::WorkspaceRepository;

    const SAMPLE_UUID: &str = "123e4567-e89b-42d3-a456-426614174000";

    fn seed() -> (tempfile::TempDir, SqliteAgentRepository, String, String) {
        let directory = tempdir().unwrap();
        let path = directory.path().join("workspace.sqlite3");
        let mut workspace = SqliteWorkspaceRepository::open(&path).unwrap();
        workspace
            .initialize(&NewWorkspace::try_new("测试", "测试教师").unwrap())
            .unwrap();
        let workspace_id = workspace
            .get_status()
            .unwrap()
            .workspace
            .unwrap()
            .workspace_id
            .clone();
        drop(workspace);

        let mut credential_repo = SqliteCredentialRepository::open(&path).unwrap();
        let credential = credential_repo
            .create(
                CredentialDraft::try_new(
                    "xAI".to_owned(),
                    "Grok".to_owned(),
                    "https://api.x.ai".to_owned(),
                    "grok-4".to_owned(),
                )
                .unwrap(),
                "sk-test".to_owned(),
            )
            .unwrap();
        drop(credential_repo);

        let repo = SqliteAgentRepository::open(&path).unwrap();
        (directory, repo, workspace_id, credential.id)
    }

    #[test]
    fn creates_and_lists_conversations() {
        let (_dir, mut repo, workspace_id, credential_id) = seed();
        let draft = ConversationDraft::try_new(
            workspace_id.clone(),
            "测试会话".to_owned(),
            credential_id.clone(),
            None,
        )
        .unwrap();
        let record = repo.create_conversation(draft).unwrap();
        assert_eq!(record.title, "测试会话");
        assert_eq!(record.status, ConversationStatus::Active);

        let list = repo.list_conversations(&workspace_id).unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].id, record.id);
    }

    #[test]
    fn appends_and_lists_messages() {
        let (_dir, mut repo, workspace_id, credential_id) = seed();
        let conversation = repo
            .create_conversation(
                ConversationDraft::try_new(
                    workspace_id,
                    "消息测试".to_owned(),
                    credential_id,
                    None,
                )
                .unwrap(),
            )
            .unwrap();
        let user_msg = MessageDraft::user(conversation.id.clone(), "你好".to_owned()).unwrap();
        let stored = repo.append_message(user_msg).unwrap();
        assert_eq!(stored.role, MessageRole::User);

        let messages = repo.list_messages(&conversation.id).unwrap();
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].content.as_deref(), Some("你好"));
    }

    #[test]
    fn creates_and_updates_invocation() {
        let (_dir, mut repo, workspace_id, credential_id) = seed();
        let conversation = repo
            .create_conversation(
                ConversationDraft::try_new(
                    workspace_id,
                    "调用测试".to_owned(),
                    credential_id,
                    None,
                )
                .unwrap(),
            )
            .unwrap();
        let message = repo
            .append_message(
                MessageDraft::assistant(
                    conversation.id.clone(),
                    None,
                    vec![ToolCall::new(
                        "call_1".to_owned(),
                        "image_generation".to_owned(),
                        r#"{"prompt":"猫"}"#.to_owned(),
                    )],
                    Some("grok-4".to_owned()),
                    Some("tool_calls".to_owned()),
                    None,
                    None,
                )
                .unwrap(),
            )
            .unwrap();

        let invocation = repo
            .create_invocation(
                ToolInvocationDraft::try_new(
                    message.id.clone(),
                    conversation.id.clone(),
                    "image_generation".to_owned(),
                    r#"{"prompt":"猫"}"#.to_owned(),
                )
                .unwrap(),
            )
            .unwrap();
        assert_eq!(invocation.status, ToolInvocationStatus::Pending);

        let updated = repo
            .update_invocation_status(
                &invocation.id,
                ToolInvocationStatus::Succeeded,
                Some(r#"{"status":"ok"}"#.to_owned()),
                None,
                None,
            )
            .unwrap();
        assert_eq!(updated.status, ToolInvocationStatus::Succeeded);
        assert!(updated.completed_at.is_some());
    }

    #[test]
    fn deletes_conversation_cascade() {
        let (_dir, mut repo, workspace_id, credential_id) = seed();
        let conversation = repo
            .create_conversation(
                ConversationDraft::try_new(
                    workspace_id,
                    "删除测试".to_owned(),
                    credential_id,
                    None,
                )
                .unwrap(),
            )
            .unwrap();
        repo.append_message(MessageDraft::user(conversation.id.clone(), "hi".to_owned()).unwrap())
            .unwrap();
        repo.delete_conversation(&conversation.id).unwrap();
        assert!(repo.get_conversation(&conversation.id).unwrap().is_none());
    }

    #[test]
    fn returns_error_when_conversation_missing() {
        let (_dir, mut repo, _workspace_id, _credential_id) = seed();
        let error = repo
            .update_conversation_status(SAMPLE_UUID, ConversationStatus::Archived)
            .unwrap_err();
        assert!(matches!(
            error,
            AgentRepositoryError::ConversationNotFound(_)
        ));
    }
}
