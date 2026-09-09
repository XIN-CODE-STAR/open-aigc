//! Creative Planner：将 CreativeBrief 转化为结构化的 CreativePlan。
//!
//! 职责：基于 Director 的意图分析，调用 LLM 生成分镜计划。
//! 输出纯声明式 CreativePlan（不含执行信息），由下游 PlanBuilder 转换为 Agent PlanStep。

use crate::{
    application::{creative_director_service::CreativeBrief, error::AppError},
    domain::{
        agent::{ChatMessage, ChatRequest},
        creative_plan::{AssetType, CreativePlan, PlanningMetadata, ShotPlan},
    },
    ports::agent_llm::AgentLlm,
};

const PLANNER_VERSION: &str = "1.0.0";

/// Creative Planner 服务（无状态，不需要持久化依赖）。
pub struct CreativePlannerService;

impl CreativePlannerService {
    pub fn new() -> Self {
        Self
    }

    /// 基于 CreativeBrief 生成结构化创作计划。
    ///
    /// 调用 LLM 生成分镜 JSON，解析为 CreativePlan。
    /// 失败时返回单镜头 fallback plan。
    pub fn plan(
        &self,
        workspace_id: &str,
        brief: &CreativeBrief,
        user_message: &str,
        model_name: &str,
        llm: &mut dyn AgentLlm,
    ) -> Result<CreativePlan, AppError> {
        let prompt = build_planner_prompt(brief, user_message);

        let chat_messages = vec![
            ChatMessage::system(prompt),
            ChatMessage::user(user_message.to_owned()),
        ];
        let request = ChatRequest::new(model_name.to_owned(), chat_messages, Vec::new());

        let response = llm
            .chat(&request)
            .map_err(|e| AppError::AgentLlmError(format!("creative planner failed: {e}")))?;

        let content = response.content.unwrap_or_default();

        match parse_creative_plan(&content, workspace_id, brief) {
            Ok(plan) => Ok(plan),
            Err(e) => {
                eprintln!("[Planner] parse failed, using fallback: {e}");
                Ok(build_fallback_plan(workspace_id, brief, user_message))
            }
        }
    }
}

impl Default for CreativePlannerService {
    fn default() -> Self {
        Self::new()
    }
}

// ─── Prompt 构建 ───

fn build_planner_prompt(brief: &CreativeBrief, user_message: &str) -> String {
    format!(
        "你是一位专业的影视分镜策划师（Storyboard Planner）。\n\n\
         根据以下创作方向，生成一个结构化的分镜计划。\n\n\
         [创作方向]\n\
         项目类型：{project_type}\n\
         目标受众：{audience}\n\
         投放平台：{platform}\n\
         视觉方向：{visual_direction}\n\
         创作策略：{strategy}\n\n\
         [用户原始请求]\n\
         {user_message}\n\n\
         请输出 JSON 格式的分镜计划：\n\n\
         ```json\n\
         {{\n\
         \"globalStyle\": \"全局风格描述\",\n\
         \"constraints\": [\"约束1\", \"约束2\"],\n\
         \"shots\": [\n\
         {{\n\
         \"index\": 1,\n\
         \"goal\": \"Opening\",\n\
         \"description\": \"镜头内容描述\",\n\
         \"durationSecs\": 5.0,\n\
         \"assetType\": \"video\",\n\
         \"style\": \"该镜头的风格（可留空使用全局）\",\n\
         \"references\": [\"参考描述\"],\n\
         \"transition\": \"cut\"\n\
         }}\n\
         ]\n\
         }}\n\
         ```\n\n\
         规则：\n\
         - 只输出 JSON，不要其他内容\n\
         - shots 数量控制在 3-8 个\n\
         - 每个 shot 的 durationSecs 在 3-10 秒之间\n\
         - assetType 只能是：image / video / audio / text\n\
         - goal 用英文：Opening / Environment / Character / Action / Emotion / Climax / Ending\n\
         - transition 可选：cut / fade / dissolve / wipe / zoom\n\
         - 总时长应匹配用户预期（如未指定，默认 20-30 秒）\n",
        project_type = brief.project_type,
        audience = brief.audience,
        platform = brief.platform,
        visual_direction = brief.visual_direction,
        strategy = brief.strategy,
        user_message = user_message,
    )
}

// ─── JSON 解析 ───

fn parse_creative_plan(
    content: &str,
    workspace_id: &str,
    brief: &CreativeBrief,
) -> Result<CreativePlan, String> {
    let json_str = extract_json(content);

    let value: serde_json::Value =
        serde_json::from_str(&json_str).map_err(|e| format!("JSON parse error: {e}"))?;

    let global_style = value
        .get("globalStyle")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_owned();

    let constraints: Vec<String> = value
        .get("constraints")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default();

    let shots_raw = value
        .get("shots")
        .and_then(|v| v.as_array())
        .ok_or("missing shots array")?;

    if shots_raw.is_empty() {
        return Err("empty shots array".to_owned());
    }

    let mut shots: Vec<ShotPlan> = Vec::new();
    for (i, shot_val) in shots_raw.iter().enumerate() {
        let asset_type_str = shot_val
            .get("assetType")
            .and_then(|v| v.as_str())
            .unwrap_or("image");
        let asset_type = match asset_type_str {
            "video" => AssetType::Video,
            "audio" => AssetType::Audio,
            "text" => AssetType::Text,
            _ => AssetType::Image,
        };

        let references: Vec<String> = shot_val
            .get("references")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default();

        shots.push(ShotPlan {
            index: (i + 1) as u8,
            goal: shot_val
                .get("goal")
                .and_then(|v| v.as_str())
                .unwrap_or("Scene")
                .to_owned(),
            description: shot_val
                .get("description")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_owned(),
            duration_secs: shot_val
                .get("durationSecs")
                .and_then(|v| v.as_f64())
                .unwrap_or(5.0) as f32,
            asset_type,
            style: shot_val
                .get("style")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_owned(),
            references,
            transition: shot_val
                .get("transition")
                .and_then(|v| v.as_str())
                .unwrap_or("cut")
                .to_owned(),
        });
    }

    Ok(CreativePlan {
        id: uuid::Uuid::new_v4().to_string(),
        version: 1,
        workspace_id: workspace_id.to_owned(),
        brief: brief.clone(),
        global_style,
        constraints,
        shots,
        metadata: PlanningMetadata {
            planner_version: PLANNER_VERSION.to_owned(),
            llm_model: String::new(),
            confidence: 0.85,
        },
        created_at: now_rfc3339(),
    })
}

/// 从 LLM 输出中提取 JSON。
fn extract_json(content: &str) -> String {
    // 尝试 ```json ... ``` 块
    if let Some(start) = content.find("```json") {
        let after = &content[start + 7..];
        if let Some(end) = after.find("```") {
            return after[..end].trim().to_owned();
        }
    }
    // 尝试 ``` ... ``` 块
    if let Some(start) = content.find("```") {
        let after = &content[start + 3..];
        if let Some(end) = after.find("```") {
            let block = after[..end].trim();
            if block.starts_with('{') {
                return block.to_owned();
            }
        }
    }
    // 裸 JSON
    if let Some(start) = content.find('{') {
        if let Some(end) = content.rfind('}') {
            if end > start {
                return content[start..=end].to_owned();
            }
        }
    }
    content.to_owned()
}

/// 降级计划：单镜头 fallback。
fn build_fallback_plan(
    workspace_id: &str,
    brief: &CreativeBrief,
    user_message: &str,
) -> CreativePlan {
    // 按字符截断，避免多字节文本切在字符边界之外 panic。
    let description = if user_message.chars().count() > 100 {
        let cut: String = user_message.chars().take(100).collect();
        format!("{cut}...")
    } else {
        user_message.to_owned()
    };

    CreativePlan {
        id: uuid::Uuid::new_v4().to_string(),
        version: 1,
        workspace_id: workspace_id.to_owned(),
        brief: brief.clone(),
        global_style: brief.visual_direction.clone(),
        constraints: Vec::new(),
        shots: vec![ShotPlan {
            index: 1,
            goal: "Scene".to_owned(),
            description,
            duration_secs: 5.0,
            asset_type: AssetType::Image,
            style: String::new(),
            references: Vec::new(),
            transition: "cut".to_owned(),
        }],
        metadata: PlanningMetadata {
            planner_version: PLANNER_VERSION.to_owned(),
            llm_model: String::new(),
            confidence: 0.3,
        },
        created_at: now_rfc3339(),
    }
}

fn now_rfc3339() -> String {
    time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_default()
}
