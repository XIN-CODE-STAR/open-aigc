#![allow(dead_code)]
//! 创作状态服务。
//!
//! 管理项目级创作上下文的读写，供 Agent 工具和 IPC 命令调用。

use std::path::PathBuf;
use std::sync::Mutex;

use crate::{
    application::error::AppError,
    domain::creative_state::{
        CharacterAsset, CreativeStateDraft, CreativeStateRecord, DecisionRecord, ReferenceAsset,
        SceneAsset, StyleTokens,
    },
    ports::creative_state_repository::CreativeStateRepository,
};

/// 创作状态服务。
pub struct CreativeStateService {
    repository: Mutex<Box<dyn CreativeStateRepository>>,
    database_path: PathBuf,
}

impl CreativeStateService {
    pub fn new(repository: impl CreativeStateRepository + 'static, database_path: PathBuf) -> Self {
        Self {
            repository: Mutex::new(Box::new(repository)),
            database_path,
        }
    }

    /// 获取工作区的创作状态（不存在则返回 None）。
    pub fn get_by_workspace(
        &self,
        workspace_id: &str,
    ) -> Result<Option<CreativeStateRecord>, AppError> {
        self.with_repo(|repo| repo.get_by_workspace(workspace_id).map_err(AppError::from))
    }

    /// 获取创作状态（按 ID）。
    pub fn get(&self, id: &str) -> Result<Option<CreativeStateRecord>, AppError> {
        self.with_repo(|repo| repo.get(id).map_err(AppError::from))
    }

    /// 创建或更新创作状态（upsert 语义：工作区已有则更新基础字段，否则创建）。
    pub fn upsert(&self, draft: CreativeStateDraft) -> Result<CreativeStateRecord, AppError> {
        draft
            .validate()
            .map_err(|e| AppError::new("validate creative state", e))?;

        self.with_repo(|repo| {
            // 检查是否已存在
            if let Some(existing) = repo
                .get_by_workspace(&draft.workspace_id)
                .map_err(AppError::from)?
            {
                let mut updated = existing;
                updated.project_name = draft.project_name;
                updated.project_type = draft.project_type;
                updated.audience = draft.audience;
                updated.platform = draft.platform;
                if !draft.style_tokens.is_empty() {
                    updated.style_tokens = draft.style_tokens;
                }
                if !draft.constraints.is_empty() {
                    updated.constraints = draft.constraints;
                }
                repo.update(&updated).map_err(AppError::from)?;
                Ok(updated)
            } else {
                repo.create(draft).map_err(AppError::from)
            }
        })
    }

    /// 更新风格 Token。
    pub fn update_style_tokens(
        &self,
        workspace_id: &str,
        tokens: StyleTokens,
    ) -> Result<CreativeStateRecord, AppError> {
        self.with_repo(|repo| {
            let mut state = Self::get_or_create(repo, workspace_id)?;
            state.style_tokens = tokens;
            repo.update(&state).map_err(AppError::from)?;
            Ok(state)
        })
    }

    /// 添加角色。
    pub fn add_character(
        &self,
        workspace_id: &str,
        character: CharacterAsset,
    ) -> Result<CreativeStateRecord, AppError> {
        self.with_repo(|repo| {
            let mut state = Self::get_or_create(repo, workspace_id)?;
            state.characters.push(character);
            repo.update(&state).map_err(AppError::from)?;
            Ok(state)
        })
    }

    /// 添加场景。
    pub fn add_scene(
        &self,
        workspace_id: &str,
        scene: SceneAsset,
    ) -> Result<CreativeStateRecord, AppError> {
        self.with_repo(|repo| {
            let mut state = Self::get_or_create(repo, workspace_id)?;
            state.scenes.push(scene);
            repo.update(&state).map_err(AppError::from)?;
            Ok(state)
        })
    }

    /// 添加参考图。
    pub fn add_reference(
        &self,
        workspace_id: &str,
        reference: ReferenceAsset,
    ) -> Result<CreativeStateRecord, AppError> {
        self.with_repo(|repo| {
            let mut state = Self::get_or_create(repo, workspace_id)?;
            state.references.push(reference);
            repo.update(&state).map_err(AppError::from)?;
            Ok(state)
        })
    }

    /// 记录创作决策。
    pub fn save_decision(
        &self,
        workspace_id: &str,
        decision: DecisionRecord,
    ) -> Result<CreativeStateRecord, AppError> {
        self.with_repo(|repo| {
            let mut state = Self::get_or_create(repo, workspace_id)?;
            state.history_decisions.push(decision);
            // 只保留最近 50 条决策
            if state.history_decisions.len() > 50 {
                let drain_count = state.history_decisions.len() - 50;
                state.history_decisions.drain(..drain_count);
            }
            repo.update(&state).map_err(AppError::from)?;
            Ok(state)
        })
    }

    /// 添加约束。
    pub fn add_constraint(
        &self,
        workspace_id: &str,
        constraint: String,
    ) -> Result<CreativeStateRecord, AppError> {
        self.with_repo(|repo| {
            let mut state = Self::get_or_create(repo, workspace_id)?;
            if !state.constraints.contains(&constraint) {
                state.constraints.push(constraint);
            }
            repo.update(&state).map_err(AppError::from)?;
            Ok(state)
        })
    }

    /// 获取风格 Token（快捷方法）。
    pub fn get_style_tokens(&self, workspace_id: &str) -> Result<StyleTokens, AppError> {
        self.with_repo(|repo| {
            match repo
                .get_by_workspace(workspace_id)
                .map_err(AppError::from)?
            {
                Some(state) => Ok(state.style_tokens),
                None => Ok(StyleTokens::default()),
            }
        })
    }

    /// 获取 Prompt 上下文（供 Agent 注入）。
    pub fn get_prompt_context(&self, workspace_id: &str) -> Result<Option<String>, AppError> {
        self.with_repo(|repo| {
            match repo
                .get_by_workspace(workspace_id)
                .map_err(AppError::from)?
            {
                Some(state) => Ok(Some(state.to_prompt_context())),
                None => Ok(None),
            }
        })
    }

    // ─── 内部方法 ───

    fn get_or_create(
        repo: &mut dyn CreativeStateRepository,
        workspace_id: &str,
    ) -> Result<CreativeStateRecord, AppError> {
        match repo
            .get_by_workspace(workspace_id)
            .map_err(AppError::from)?
        {
            Some(state) => Ok(state),
            None => {
                let draft = CreativeStateDraft {
                    workspace_id: workspace_id.to_owned(),
                    project_name: String::new(),
                    project_type: String::new(),
                    audience: String::new(),
                    platform: String::new(),
                    style_tokens: StyleTokens::default(),
                    constraints: Vec::new(),
                };
                repo.create(draft).map_err(AppError::from)
            }
        }
    }

    fn with_repo<T>(
        &self,
        op: impl FnOnce(&mut dyn CreativeStateRepository) -> Result<T, AppError>,
    ) -> Result<T, AppError> {
        let mut repo = self
            .repository
            .lock()
            .map_err(|_| AppError::StateUnavailable)?;
        op(repo.as_mut())
    }
}
