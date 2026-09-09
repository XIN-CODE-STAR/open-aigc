use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use rusqlite::{params, Connection};

use crate::{
    adapters::sqlite::database::open_database,
    application::credential_service::CredentialService,
    application::error::AppError,
    domain::{
        credentials::CredentialRecord,
        model_router::{
            ApprovalPolicy, ModelCandidate, ModelCapabilityRecord, RouterError, RoutingDecision,
            RoutingRequest, RoutingStrategy, RoutingTaskType,
        },
    },
};

/// Cognitive Router：按创作认知阶段分配模型资源。
///
/// Phase 1 的核心边界：
/// - 能力注册表来自 SQLite，不再写死在 Rust 代码中。
/// - LLM 只描述任务，Router 根据任务类型、能力向量、硬约束和失败日志选择模型。
/// - 当前实现是线性工作流前置能力；DAG / Feedback Loop 在 Phase 2 扩展。
pub struct ModelRouterService {
    credential_service: CredentialService,
    database_path: PathBuf,
}

impl ModelRouterService {
    pub fn new(credential_service: CredentialService, database_path: PathBuf) -> Self {
        Self {
            credential_service,
            database_path,
        }
    }

    /// 根据请求选择最优模型。
    pub fn route(&self, request: &RoutingRequest) -> Result<RoutingDecision, AppError> {
        let mut candidates = self.build_candidates(request, true)?;

        if candidates.is_empty() {
            return Err(AppError::new(
                "no available model",
                RouterError::NoAvailableModel {
                    task_type: request.task_type.as_str().to_owned(),
                },
            ));
        }

        if let Some(preferred) = request.preferred_model.as_ref() {
            if let Some(idx) = candidates.iter().position(|candidate| {
                candidate.model_name.eq_ignore_ascii_case(preferred)
                    || candidate.display_name.eq_ignore_ascii_case(preferred)
            }) {
                let selected = candidates.remove(idx);
                return Ok(RoutingDecision {
                    alternatives: candidates,
                    reason: format!("用户指定模型 {preferred}，Router 已验证其可用于当前任务。"),
                    strategy: RoutingStrategy::UserSpecified,
                    requires_confirmation: request.task_type.is_high_cost(),
                    approval_policy: approval_policy_for(request.task_type),
                    estimated_cost: None,
                    selected,
                });
            }
        }

        sort_candidates(&mut candidates, request.strategy);
        let selected = candidates.remove(0);
        let reason = format!(
            "Cognitive Router 根据 {} 阶段、能力向量相似度、运行失败修正和 {:?} 策略选择 {}，综合评分 {:.2}。",
            request.task_type.as_str(),
            request.strategy,
            selected.display_name,
            selected.overall_score
        );

        Ok(RoutingDecision {
            selected,
            alternatives: candidates,
            reason,
            strategy: request.strategy,
            requires_confirmation: request.task_type.is_high_cost(),
            approval_policy: approval_policy_for(request.task_type),
            estimated_cost: None,
        })
    }

    /// 列出模型候选。用于前端解释 Router 的候选来源。
    pub fn list_available_models(
        &self,
        task_type: Option<RoutingTaskType>,
    ) -> Result<Vec<ModelCandidate>, AppError> {
        let task_types: Vec<RoutingTaskType> = match task_type {
            Some(task) => vec![task],
            None => vec![
                RoutingTaskType::TextGeneration,
                RoutingTaskType::ImageGeneration,
                RoutingTaskType::VideoGeneration,
                RoutingTaskType::TextToSpeech,
                RoutingTaskType::VisionEvaluation,
                RoutingTaskType::ContentGuard,
                RoutingTaskType::FeedbackParsing,
                RoutingTaskType::CharacterConsistency,
                RoutingTaskType::StyleConsistency,
            ],
        };

        let mut candidates = Vec::new();
        for task in task_types {
            let request = RoutingRequest {
                task_type: task,
                strategy: RoutingStrategy::Balanced,
                preferred_provider: None,
                preferred_model: None,
                budget_limit: None,
                requires_reference: false,
                context: None,
            };
            candidates.extend(self.build_candidates(&request, false)?);
        }
        sort_candidates(&mut candidates, RoutingStrategy::Balanced);
        Ok(candidates)
    }

    /// 记录模型使用结果，写回 Capability Registry 的 failure_log_json。
    pub fn record_outcome(
        &self,
        provider_id: &str,
        model_name: &str,
        task_type: RoutingTaskType,
        success: bool,
    ) -> Result<(), AppError> {
        let connection = open_router_database(&self.database_path)?;
        let mut records = load_capabilities(&connection, Some(task_type))?;
        let Some(record) = records.iter_mut().find(|record| {
            provider_matches(&record.provider_id, provider_id)
                && model_matches(&record.model_name, model_name)
        }) else {
            return Ok(());
        };

        let key = task_type.as_str().to_owned();
        let stats = record.failure_log.entry(key).or_default();
        stats.attempts += 1;
        if !success {
            stats.failures += 1;
            stats.score_delta = (stats.score_delta - 0.02).max(-0.5);
        } else {
            stats.score_delta = (stats.score_delta + 0.005).min(0.1);
        }

        let failure_log_json = serde_json::to_string(&record.failure_log)
            .map_err(|error| AppError::new("serialize model failure log", error))?;
        connection
            .execute(
                "UPDATE model_capabilities
                 SET failure_log_json = ?1, updated_at = datetime('now')
                 WHERE id = ?2",
                params![failure_log_json, record.id],
            )
            .map_err(|error| AppError::new("update model capability outcome", error))?;
        Ok(())
    }

    fn build_candidates(
        &self,
        request: &RoutingRequest,
        only_available: bool,
    ) -> Result<Vec<ModelCandidate>, AppError> {
        let connection = open_router_database(&self.database_path)?;
        let entries = load_capabilities(&connection, Some(request.task_type))?;
        let credentials = self.credential_service.list().unwrap_or_default();
        let requirement_vector = requirement_vector(request);

        let mut candidates = Vec::new();
        for entry in entries {
            if !entry.enabled || !passes_hard_constraints(&entry, request) {
                continue;
            }

            let matching_credentials = matching_credentials(&entry, &credentials);
            let has_runtime_credential = !matching_credentials.is_empty();
            if only_available && !has_runtime_credential {
                continue;
            }

            if has_runtime_credential {
                for credential in matching_credentials {
                    candidates.push(score_entry(
                        &entry,
                        Some(credential),
                        request,
                        &requirement_vector,
                    ));
                }
            } else {
                candidates.push(score_entry(&entry, None, request, &requirement_vector));
            }
        }

        if let Some(preferred_provider) = request.preferred_provider.as_ref() {
            candidates
                .retain(|candidate| provider_matches(&candidate.provider_id, preferred_provider));
        }

        Ok(candidates)
    }
}

impl crate::ports::reloadable::Reloadable for ModelRouterService {
    fn reload(&self, database_path: &Path) -> Result<(), AppError> {
        // Router 不持有长期 SQLite 连接；下次 route/list 会用新路径打开数据库。
        if database_path != self.database_path {
            // ServiceReloader 目前传入同一路径。若未来支持工作空间切换，
            // 应创建新的 ModelRouterService 实例，而不是原地修改不可变路径。
        }
        Ok(())
    }
}

fn open_router_database(database_path: &Path) -> Result<Connection, AppError> {
    open_database(database_path).map_err(AppError::from)
}

fn load_capabilities(
    connection: &Connection,
    task_type: Option<RoutingTaskType>,
) -> Result<Vec<ModelCapabilityRecord>, AppError> {
    let sql = if task_type.is_some() {
        "SELECT id, provider_id, model_name, display_name, task_type,
                capabilities_json, prerequisites_json, constraints_json, failure_log_json,
                cost_score, speed_score, quality_score, enabled
         FROM model_capabilities
         WHERE task_type = ?1
         ORDER BY provider_id, model_name"
    } else {
        "SELECT id, provider_id, model_name, display_name, task_type,
                capabilities_json, prerequisites_json, constraints_json, failure_log_json,
                cost_score, speed_score, quality_score, enabled
         FROM model_capabilities
         ORDER BY provider_id, model_name"
    };

    let parse_row = |row: &rusqlite::Row<'_>| -> Result<ModelCapabilityRecord, rusqlite::Error> {
        let task_type_raw: String = row.get(4)?;
        let task_type = RoutingTaskType::parse(&task_type_raw).map_err(|error| {
            rusqlite::Error::FromSqlConversionFailure(
                4,
                rusqlite::types::Type::Text,
                Box::new(error),
            )
        })?;
        let capabilities_json: String = row.get(5)?;
        let prerequisites_json: String = row.get(6)?;
        let constraints_json: String = row.get(7)?;
        let failure_log_json: String = row.get(8)?;

        Ok(ModelCapabilityRecord {
            id: row.get(0)?,
            provider_id: row.get(1)?,
            model_name: row.get(2)?,
            display_name: row.get(3)?,
            task_type,
            capabilities: serde_json::from_str(&capabilities_json).unwrap_or_default(),
            prerequisites: serde_json::from_str(&prerequisites_json).unwrap_or_default(),
            constraints: serde_json::from_str(&constraints_json).unwrap_or_default(),
            failure_log: serde_json::from_str(&failure_log_json).unwrap_or_default(),
            cost_score: row.get(9)?,
            speed_score: row.get(10)?,
            quality_score: row.get(11)?,
            enabled: row.get::<_, i64>(12)? == 1,
        })
    };

    if let Some(task) = task_type {
        let mut statement = connection
            .prepare(sql)
            .map_err(|error| AppError::new("prepare model capability query", error))?;
        let rows = statement
            .query_map([task.as_str()], parse_row)
            .map_err(|error| AppError::new("query model capabilities", error))?;
        collect_rows(rows)
    } else {
        let mut statement = connection
            .prepare(sql)
            .map_err(|error| AppError::new("prepare model capability query", error))?;
        let rows = statement
            .query_map([], parse_row)
            .map_err(|error| AppError::new("query model capabilities", error))?;
        collect_rows(rows)
    }
}

fn collect_rows<T>(
    rows: rusqlite::MappedRows<'_, impl FnMut(&rusqlite::Row<'_>) -> Result<T, rusqlite::Error>>,
) -> Result<Vec<T>, AppError> {
    let mut records = Vec::new();
    for row in rows {
        records.push(row.map_err(|error| AppError::new("read model capability row", error))?);
    }
    Ok(records)
}

fn matching_credentials<'a>(
    entry: &ModelCapabilityRecord,
    credentials: &'a [CredentialRecord],
) -> Vec<&'a CredentialRecord> {
    credentials
        .iter()
        .filter(|credential| {
            credential.enabled
                && provider_matches(&entry.provider_id, &credential.provider_name)
                && (model_matches(&entry.model_name, &credential.model_name)
                    || provider_matches(&entry.provider_id, &credential.provider_name))
        })
        .collect()
}

fn score_entry(
    entry: &ModelCapabilityRecord,
    credential: Option<&CredentialRecord>,
    request: &RoutingRequest,
    requirement_vector: &BTreeMap<String, f64>,
) -> ModelCandidate {
    let similarity = cosine_similarity(requirement_vector, &entry.capabilities);
    let failure_delta = failure_delta(entry, request.task_type);
    let capability_score = (similarity + failure_delta).clamp(0.0, 1.0);
    let available = credential.is_some();

    let quality_score = (entry.quality_score + failure_delta).clamp(0.0, 1.0);
    let provider_id = credential
        .map(|c| c.provider_name.clone())
        .unwrap_or_else(|| entry.provider_id.clone());
    let model_name = credential
        .map(|c| c.model_name.clone())
        .unwrap_or_else(|| entry.model_name.clone());
    let display_name = credential
        .map(|c| format!("{} / {}", c.display_name, c.model_name))
        .unwrap_or_else(|| entry.display_name.clone());

    let overall_score = weighted_score(
        request.strategy,
        capability_score,
        entry.cost_score,
        entry.speed_score,
        quality_score,
    );

    ModelCandidate {
        provider_id,
        model_name,
        display_name,
        capability_score,
        cost_score: entry.cost_score,
        speed_score: entry.speed_score,
        quality_score,
        overall_score,
        available,
        remaining_quota: -1,
        health_status: format!("ok · {}", entry.task_type.as_str()),
    }
}

fn requirement_vector(request: &RoutingRequest) -> BTreeMap<String, f64> {
    let mut vector = BTreeMap::new();
    match request.task_type {
        RoutingTaskType::TextGeneration => {
            vector.insert("planning".to_owned(), 0.9);
            vector.insert("scriptwriting".to_owned(), 0.85);
            vector.insert("visual_reasoning".to_owned(), 0.65);
            vector.insert("prompt_adherence".to_owned(), 0.7);
        }
        RoutingTaskType::ImageGeneration => {
            vector.insert("prompt_adherence".to_owned(), 0.9);
            vector.insert("image_quality".to_owned(), 0.9);
            vector.insert("visual_reasoning".to_owned(), 0.6);
            vector.insert("character_consistency".to_owned(), 0.55);
        }
        RoutingTaskType::VideoGeneration => {
            vector.insert("prompt_adherence".to_owned(), 0.75);
            vector.insert("camera_motion".to_owned(), 0.95);
            vector.insert("character_consistency".to_owned(), 0.7);
            vector.insert("image_quality".to_owned(), 0.55);
        }
        RoutingTaskType::TextToSpeech => {
            vector.insert("prompt_adherence".to_owned(), 0.65);
        }
        RoutingTaskType::VisionEvaluation
        | RoutingTaskType::ContentGuard
        | RoutingTaskType::FeedbackParsing
        | RoutingTaskType::CharacterConsistency
        | RoutingTaskType::StyleConsistency => {
            vector.insert("visual_reasoning".to_owned(), 0.85);
            vector.insert("prompt_adherence".to_owned(), 0.75);
        }
    }

    if request.requires_reference {
        vector.insert("character_consistency".to_owned(), 0.85);
    }

    if let Some(context) = request.context.as_deref() {
        let normalized = normalize(context);
        if normalized.contains("央视")
            || normalized.contains("纪录片")
            || normalized.contains("documentary")
        {
            bump(&mut vector, "scriptwriting", 0.1);
            bump(&mut vector, "visual_reasoning", 0.1);
        }
        if normalized.contains("分镜") || normalized.contains("storyboard") {
            bump(&mut vector, "planning", 0.1);
            bump(&mut vector, "prompt_adherence", 0.1);
        }
        if normalized.contains("角色") || normalized.contains("一致") {
            bump(&mut vector, "character_consistency", 0.2);
        }
        if normalized.contains("运镜")
            || normalized.contains("镜头")
            || normalized.contains("camera")
        {
            bump(&mut vector, "camera_motion", 0.2);
        }
    }

    vector
}

fn bump(vector: &mut BTreeMap<String, f64>, key: &str, amount: f64) {
    let value = vector.entry(key.to_owned()).or_insert(0.0);
    *value = (*value + amount).min(1.0);
}

fn passes_hard_constraints(entry: &ModelCapabilityRecord, request: &RoutingRequest) -> bool {
    if request.requires_reference && !entry.constraints.supports_reference {
        return false;
    }
    true
}

fn failure_delta(entry: &ModelCapabilityRecord, task_type: RoutingTaskType) -> f64 {
    let mut delta = entry
        .failure_log
        .get(task_type.as_str())
        .map(|stats| stats.score_delta)
        .unwrap_or(0.0);
    for stats in entry.failure_log.values() {
        if stats.attempts > 0 {
            let failure_rate = stats.failures as f64 / stats.attempts as f64;
            delta -= failure_rate.min(0.4) * 0.08;
        }
    }
    delta.clamp(-0.5, 0.1)
}

fn cosine_similarity(left: &BTreeMap<String, f64>, right: &BTreeMap<String, f64>) -> f64 {
    let keys: BTreeSet<_> = left.keys().chain(right.keys()).collect();
    let mut dot = 0.0;
    let mut left_norm = 0.0;
    let mut right_norm = 0.0;

    for key in keys {
        let l = *left.get(key).unwrap_or(&0.0);
        let r = *right.get(key).unwrap_or(&0.0);
        dot += l * r;
        left_norm += l * l;
        right_norm += r * r;
    }

    if left_norm == 0.0 || right_norm == 0.0 {
        return 0.0;
    }
    (dot / (left_norm.sqrt() * right_norm.sqrt())).clamp(0.0, 1.0)
}

fn weighted_score(
    strategy: RoutingStrategy,
    capability_score: f64,
    cost_score: f64,
    speed_score: f64,
    quality_score: f64,
) -> f64 {
    match strategy {
        RoutingStrategy::CostOptimized => {
            cost_score * 0.45 + capability_score * 0.25 + quality_score * 0.2 + speed_score * 0.1
        }
        RoutingStrategy::QualityFirst => {
            quality_score * 0.45 + capability_score * 0.4 + speed_score * 0.1 + cost_score * 0.05
        }
        RoutingStrategy::SpeedFirst => {
            speed_score * 0.45 + capability_score * 0.25 + quality_score * 0.2 + cost_score * 0.1
        }
        RoutingStrategy::Balanced | RoutingStrategy::UserSpecified => {
            capability_score * 0.35 + quality_score * 0.3 + speed_score * 0.18 + cost_score * 0.17
        }
    }
    .clamp(0.0, 1.0)
}

fn sort_candidates(candidates: &mut [ModelCandidate], strategy: RoutingStrategy) {
    candidates.sort_by(|a, b| {
        let ordering = match strategy {
            RoutingStrategy::CostOptimized => b.cost_score.partial_cmp(&a.cost_score),
            RoutingStrategy::QualityFirst => b.quality_score.partial_cmp(&a.quality_score),
            RoutingStrategy::SpeedFirst => b.speed_score.partial_cmp(&a.speed_score),
            RoutingStrategy::Balanced | RoutingStrategy::UserSpecified => {
                b.overall_score.partial_cmp(&a.overall_score)
            }
        };
        ordering.unwrap_or(std::cmp::Ordering::Equal)
    });
}

fn approval_policy_for(task_type: RoutingTaskType) -> ApprovalPolicy {
    match task_type {
        RoutingTaskType::ImageGeneration | RoutingTaskType::VideoGeneration => {
            ApprovalPolicy::ConfirmCost
        }
        _ => ApprovalPolicy::Auto,
    }
}

fn provider_matches(left: &str, right: &str) -> bool {
    let left_aliases = provider_aliases(left);
    let right_normalized = normalize(right);
    left_aliases
        .iter()
        .any(|alias| right_normalized.contains(alias) || alias.contains(&right_normalized))
}

fn model_matches(left: &str, right: &str) -> bool {
    let left = normalize(left);
    let right = normalize(right);
    left == right || left.contains(&right) || right.contains(&left)
}

fn provider_aliases(value: &str) -> Vec<String> {
    let normalized = normalize(value);
    let mut aliases = vec![normalized.clone()];
    match normalized.as_str() {
        "claude" | "anthropic" => {
            aliases.push("anthropic".to_owned());
            aliases.push("claude".to_owned());
        }
        "openai" | "omni" | "gpt" => {
            aliases.push("openai".to_owned());
            aliases.push("omni".to_owned());
            aliases.push("gpt".to_owned());
        }
        "seedance" | "doubao" | "volcengine" => {
            aliases.push("seedance".to_owned());
            aliases.push("doubao".to_owned());
            aliases.push("volcengine".to_owned());
        }
        _ => {}
    }
    aliases.sort();
    aliases.dedup();
    aliases
}

fn normalize(value: &str) -> String {
    value
        .trim()
        .to_lowercase()
        .replace([' ', '_', '-', '/', '·'], "")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cosine_similarity_prefers_matching_dimensions() {
        let request = RoutingRequest {
            task_type: RoutingTaskType::VideoGeneration,
            strategy: RoutingStrategy::Balanced,
            preferred_provider: None,
            preferred_model: None,
            budget_limit: None,
            requires_reference: false,
            context: Some("需要电影级运镜".to_owned()),
        };
        let vector = requirement_vector(&request);
        assert!(vector.get("camera_motion").copied().unwrap_or_default() > 0.9);
    }

    #[test]
    fn provider_aliases_match_claude_to_anthropic() {
        assert!(provider_matches("anthropic", "Claude"));
        assert!(provider_matches("seedance", "Seedance"));
    }

    #[test]
    fn high_cost_tasks_require_cost_confirmation() {
        assert_eq!(
            approval_policy_for(RoutingTaskType::VideoGeneration),
            ApprovalPolicy::ConfirmCost
        );
        assert_eq!(
            approval_policy_for(RoutingTaskType::TextGeneration),
            ApprovalPolicy::Auto
        );
    }
}
