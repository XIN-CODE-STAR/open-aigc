#![allow(dead_code)]
//! Agent 受控工具目录。
//!
//! 工具按类型分为：只读/草稿（无需确认）、写操作（需要确认）、成本操作（必须确认）。
//! 每个工具声明输入/输出 schema、副作用、权限和确认要求。

use serde::{Deserialize, Serialize};

/// 工具类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ToolCategory {
    /// 只读操作，无副作用。
    ReadOnly,
    /// 草稿操作，生成临时内容。
    Draft,
    /// 写操作，修改数据。
    Write,
    /// 涉及成本的操作（消耗额度）。
    Costly,
}

/// 工具定义。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolCatalogEntry {
    /// 工具唯一标识。
    pub id: String,
    /// 展示名称。
    pub name: String,
    /// 工具描述（面向学生，简洁易懂）。
    pub description: String,
    /// 工具类别。
    pub category: ToolCategory,
    /// 是否需要人工确认。
    pub requires_confirmation: bool,
    /// 输入参数 JSON Schema。
    pub input_schema: serde_json::Value,
    /// 输出结果 JSON Schema。
    pub output_schema: serde_json::Value,
    /// 副作用描述（如"创建生成任务"、"修改数据库"）。
    pub side_effects: Vec<String>,
}

/// 工具注册表。
pub struct ToolCatalog {
    entries: Vec<ToolCatalogEntry>,
}

impl ToolCatalog {
    /// 创建默认的创意工坊工具目录。
    pub fn creative_workshop() -> Self {
        Self {
            entries: vec![
                ToolCatalogEntry {
                    id: "draft_story_outline".into(),
                    name: "生成故事大纲".into(),
                    description: "根据教学主题或故事方向，生成故事大纲草稿。不会修改任何数据。"
                        .into(),
                    category: ToolCategory::Draft,
                    requires_confirmation: false,
                    input_schema: serde_json::json!({
                        "type": "object",
                        "properties": {
                            "theme": {"type": "string", "description": "教学主题或故事方向"},
                            "style": {"type": "string", "description": "风格偏好（可选）"}
                        },
                        "required": ["theme"]
                    }),
                    output_schema: serde_json::json!({
                        "type": "object",
                        "properties": {
                            "outline": {"type": "string", "description": "故事大纲"}
                        }
                    }),
                    side_effects: vec![],
                },
                ToolCatalogEntry {
                    id: "draft_character_profiles".into(),
                    name: "生成角色设定".into(),
                    description: "根据故事大纲，生成角色设定草稿。不会修改任何数据。".into(),
                    category: ToolCategory::Draft,
                    requires_confirmation: false,
                    input_schema: serde_json::json!({
                        "type": "object",
                        "properties": {
                            "outline": {"type": "string", "description": "故事大纲"},
                            "count": {"type": "integer", "description": "角色数量", "default": 3}
                        },
                        "required": ["outline"]
                    }),
                    output_schema: serde_json::json!({
                        "type": "object",
                        "properties": {
                            "characters": {"type": "array", "items": {"type": "object"}}
                        }
                    }),
                    side_effects: vec![],
                },
                ToolCatalogEntry {
                    id: "draft_shot_list".into(),
                    name: "生成分镜表".into(),
                    description: "根据剧本段落，生成分镜/镜头清单草稿。不会修改任何数据。".into(),
                    category: ToolCategory::Draft,
                    requires_confirmation: false,
                    input_schema: serde_json::json!({
                        "type": "object",
                        "properties": {
                            "scene_summary": {"type": "string", "description": "场景摘要"},
                            "characters": {"type": "array", "items": {"type": "string"}, "description": "出场角色"}
                        },
                        "required": ["scene_summary"]
                    }),
                    output_schema: serde_json::json!({
                        "type": "object",
                        "properties": {
                            "shots": {"type": "array", "items": {"type": "object"}}
                        }
                    }),
                    side_effects: vec![],
                },
                ToolCatalogEntry {
                    id: "render_provider_prompt".into(),
                    name: "转换为 Provider Prompt".into(),
                    description: "将镜头描述转换为指定 Provider 可用的 Prompt。不会修改任何数据。"
                        .into(),
                    category: ToolCategory::ReadOnly,
                    requires_confirmation: false,
                    input_schema: serde_json::json!({
                        "type": "object",
                        "properties": {
                            "shot_description": {"type": "string", "description": "镜头描述"},
                            "provider": {"type": "string", "description": "目标 Provider"},
                            "style_guide": {"type": "string", "description": "风格指南"}
                        },
                        "required": ["shot_description", "provider"]
                    }),
                    output_schema: serde_json::json!({
                        "type": "object",
                        "properties": {
                            "prompt": {"type": "string"},
                            "negative_prompt": {"type": "string"}
                        }
                    }),
                    side_effects: vec![],
                },
                ToolCatalogEntry {
                    id: "create_generation_task".into(),
                    name: "创建生成任务".into(),
                    description: "为单个镜头创建 AI 生成任务。需要确认后执行。".into(),
                    category: ToolCategory::Costly,
                    requires_confirmation: true,
                    input_schema: serde_json::json!({
                        "type": "object",
                        "properties": {
                            "shot_id": {"type": "string", "description": "镜头 ID"},
                            "prompt": {"type": "string", "description": "生成 Prompt"},
                            "provider": {"type": "string", "description": "目标 Provider"},
                            "model": {"type": "string", "description": "模型名称"}
                        },
                        "required": ["shot_id", "prompt", "provider"]
                    }),
                    output_schema: serde_json::json!({
                        "type": "object",
                        "properties": {
                            "task_id": {"type": "string"},
                            "attempt_id": {"type": "string"}
                        }
                    }),
                    side_effects: vec!["创建生成任务".into(), "消耗 Provider 额度".into()],
                },
                ToolCatalogEntry {
                    id: "create_batch_generation_tasks".into(),
                    name: "批量创建生成任务".into(),
                    description: "为多个镜头批量创建 AI 生成任务。需要确认任务数、账号和预计消耗。"
                        .into(),
                    category: ToolCategory::Costly,
                    requires_confirmation: true,
                    input_schema: serde_json::json!({
                        "type": "object",
                        "properties": {
                            "shot_ids": {"type": "array", "items": {"type": "string"}, "description": "镜头 ID 列表"},
                            "provider": {"type": "string", "description": "目标 Provider"}
                        },
                        "required": ["shot_ids", "provider"]
                    }),
                    output_schema: serde_json::json!({
                        "type": "object",
                        "properties": {
                            "task_ids": {"type": "array", "items": {"type": "string"}}
                        }
                    }),
                    side_effects: vec!["批量创建生成任务".into(), "批量消耗 Provider 额度".into()],
                },
                ToolCatalogEntry {
                    id: "attach_asset_to_shot".into(),
                    name: "关联素材到镜头".into(),
                    description: "将生成结果或参考素材关联到指定镜头。需要确认。".into(),
                    category: ToolCategory::Write,
                    requires_confirmation: true,
                    input_schema: serde_json::json!({
                        "type": "object",
                        "properties": {
                            "shot_id": {"type": "string", "description": "镜头 ID"},
                            "asset_id": {"type": "string", "description": "素材 ID"},
                            "kind": {"type": "string", "description": "关联类型（参考图/生成结果/首帧/尾帧）"}
                        },
                        "required": ["shot_id", "asset_id", "kind"]
                    }),
                    output_schema: serde_json::json!({
                        "type": "object",
                        "properties": {
                            "binding_id": {"type": "string"}
                        }
                    }),
                    side_effects: vec!["创建镜头-素材关联".into()],
                },
                ToolCatalogEntry {
                    id: "search_assets".into(),
                    name: "检索素材".into(),
                    description: "在项目素材库中搜索符合条件的素材。不会修改任何数据。".into(),
                    category: ToolCategory::ReadOnly,
                    requires_confirmation: false,
                    input_schema: serde_json::json!({
                        "type": "object",
                        "properties": {
                            "query": {"type": "string", "description": "搜索关键词"},
                            "kind": {"type": "string", "description": "素材类型（可选）"}
                        },
                        "required": ["query"]
                    }),
                    output_schema: serde_json::json!({
                        "type": "object",
                        "properties": {
                            "assets": {"type": "array", "items": {"type": "object"}}
                        }
                    }),
                    side_effects: vec![],
                },
                ToolCatalogEntry {
                    id: "check_provider_health".into(),
                    name: "检查账号状态".into(),
                    description: "检查指定 Provider 的账号可用性。不会修改任何数据。".into(),
                    category: ToolCategory::ReadOnly,
                    requires_confirmation: false,
                    input_schema: serde_json::json!({
                        "type": "object",
                        "properties": {
                            "provider": {"type": "string", "description": "Provider 名称"}
                        },
                        "required": ["provider"]
                    }),
                    output_schema: serde_json::json!({
                        "type": "object",
                        "properties": {
                            "status": {"type": "string", "enum": ["ok", "degraded", "expired", "failed", "unknown"]},
                            "message": {"type": "string"}
                        }
                    }),
                    side_effects: vec![],
                },
            ],
        }
    }

    /// 获取所有工具定义。
    pub fn entries(&self) -> &[ToolCatalogEntry] {
        &self.entries
    }

    /// 按 ID 查找工具。
    pub fn get(&self, id: &str) -> Option<&ToolCatalogEntry> {
        self.entries.iter().find(|e| e.id == id)
    }

    /// 列出需要确认的工具。
    pub fn confirmation_required(&self) -> Vec<&ToolCatalogEntry> {
        self.entries
            .iter()
            .filter(|e| e.requires_confirmation)
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn creative_workshop_has_all_tools() {
        let catalog = ToolCatalog::creative_workshop();
        assert_eq!(catalog.entries().len(), 9);
    }

    #[test]
    fn costly_tools_require_confirmation() {
        let catalog = ToolCatalog::creative_workshop();
        let costly: Vec<_> = catalog
            .entries()
            .iter()
            .filter(|e| e.category == ToolCategory::Costly)
            .collect();
        assert!(costly.iter().all(|e| e.requires_confirmation));
    }

    #[test]
    fn draft_tools_dont_require_confirmation() {
        let catalog = ToolCatalog::creative_workshop();
        let drafts: Vec<_> = catalog
            .entries()
            .iter()
            .filter(|e| e.category == ToolCategory::Draft)
            .collect();
        assert!(drafts.iter().all(|e| !e.requires_confirmation));
    }

    #[test]
    fn find_tool_by_id() {
        let catalog = ToolCatalog::creative_workshop();
        let tool = catalog.get("draft_story_outline").unwrap();
        assert_eq!(tool.name, "生成故事大纲");
    }
}
