//! ScriptParserService — 脚本解析服务。
//!
//! 从 builtin_tools.rs 的 execute_parse_script 中提取的启发式解析逻辑。
//! 输入：纯文本剧本 → 输出：结构化 ScriptPlan。
//!
//! 设计原则：
//! - 纯文本处理（无 I/O、无数据库访问）
//! - 返回强类型 ScriptPlan（不是 ad-hoc JSON）
//! - 分离"解析"和"Prompt 生成"（PromptCompiler 负责后者）

use crate::domain::script_plan::{
    PlannedScene, PlannedShot, ScriptPlan, ScriptPlanDraft, ScriptPlanError, MAX_SCENES,
};

// ──────────────────────────────────────────────────────────────────
// Constants
// ──────────────────────────────────────────────────────────────────

/// 场景边界标记（中英文）。
const SCENE_MARKERS: &[&str] = &[
    "场景", "内景", "外景", "Scene", "scene", "INT.", "EXT.", "int.", "ext.", "第.*幕",
];

// ──────────────────────────────────────────────────────────────────
// ScriptParserService
// ──────────────────────────────────────────────────────────────────

pub struct ScriptParserService;

impl ScriptParserService {
    /// 解析剧本文本为 ScriptPlan。
    ///
    /// 使用启发式方法：按场景标记分割文本，为每个场景生成 1-3 个镜头，
    /// 推断镜头类型和运镜方式，生成英文 Prompt。
    pub fn parse(
        &self,
        script_text: &str,
        style: Option<&str>,
        max_scenes: Option<usize>,
    ) -> Result<ScriptPlan, ScriptPlanError> {
        let script_text = script_text.trim();
        if script_text.is_empty() {
            return Err(ScriptPlanError::EmptyScript);
        }

        let max_scenes = max_scenes.unwrap_or(10).min(MAX_SCENES);

        // Step 1: Split scenes
        let raw_scenes = Self::split_scenes(script_text);

        // Step 2: Generate shots for each scene
        let mut scenes: Vec<PlannedScene> = Vec::new();

        for (i, scene_text) in raw_scenes.iter().enumerate().take(max_scenes) {
            let scene_index = (i + 1) as u32;
            let scene_title = Self::extract_scene_title(scene_text, scene_index as usize);
            let location = Self::detect_location(scene_text);
            let summary = scene_text.chars().take(200).collect::<String>();

            // Split scene text into sentences for shot generation
            let sentences: Vec<&str> = scene_text
                .split(|c: char| {
                    c == '。' || c == '！' || c == '？' || c == '.' || c == '!' || c == '?'
                })
                .map(|s| s.trim())
                .filter(|s| !s.is_empty())
                .collect();

            let shot_count = sentences.len().clamp(1, 3);
            let mut shots: Vec<PlannedShot> = Vec::new();

            for j in 0..shot_count {
                let shot_index = (j + 1) as u32;
                let sentence = sentences.get(j).copied().unwrap_or(scene_text);
                let shot_type = Self::infer_shot_type(sentence);
                let camera_motion = Self::infer_camera_motion(sentence);
                let visual_intent =
                    Self::generate_visual_intent(sentence, style.unwrap_or(""), &shot_type);

                shots.push(PlannedShot {
                    index: shot_index,
                    description: sentence.to_string(),
                    shot_type: Some(shot_type),
                    camera_motion: Some(camera_motion),
                    visual_intent: Some(visual_intent),
                    duration: None,
                    character_refs: Self::extract_character_refs(sentence),
                    style_override: None,
                });
            }

            scenes.push(PlannedScene {
                index: scene_index,
                title: scene_title,
                summary: Some(summary),
                location: Some(location),
                shots,
            });
        }

        let suggested_title = Self::extract_suggested_title(script_text);

        ScriptPlanDraft {
            conversation_id: None,
            title: Some(suggested_title),
            style: style.map(|s| s.to_owned()),
            scenes,
        }
        .try_into_plan()
    }

    // ── Scene splitting ──

    /// Split script text into scene segments.
    fn split_scenes(text: &str) -> Vec<String> {
        let mut scenes: Vec<String> = Vec::new();
        let mut current = String::new();

        for line in text.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                if !current.is_empty() {
                    current.push('\n');
                }
                continue;
            }

            let is_scene_marker = Self::is_scene_boundary(trimmed);
            if is_scene_marker && !current.trim().is_empty() {
                scenes.push(current.trim().to_owned());
                current = String::new();
            }
            current.push_str(trimmed);
            current.push('\n');
        }
        if !current.trim().is_empty() {
            scenes.push(current.trim().to_owned());
        }

        // Fallback: split by double newline if no scene markers found
        if scenes.len() <= 1 {
            let paragraphs: Vec<&str> = text
                .split("\n\n")
                .map(|p| p.trim())
                .filter(|p| !p.is_empty())
                .collect();
            if paragraphs.len() > 1 {
                return paragraphs.into_iter().map(|p| p.to_owned()).collect();
            }
        }

        if scenes.is_empty() {
            scenes.push(text.to_owned());
        }
        scenes
    }

    /// Check if a line is a scene boundary.
    fn is_scene_boundary(line: &str) -> bool {
        for marker in SCENE_MARKERS {
            if line.contains(marker) {
                return true;
            }
        }
        if line.starts_with("第") && line.contains("幕") {
            return true;
        }
        false
    }

    // ── Scene title extraction ──

    fn extract_scene_title(scene_text: &str, index: usize) -> String {
        let first_line = scene_text.lines().next().unwrap_or("").trim();
        if Self::is_scene_boundary(first_line) {
            for (i, ch) in first_line.char_indices() {
                if ch == '：' || ch == ':' {
                    let title = first_line[i + ch.len_utf8()..].trim();
                    if !title.is_empty() {
                        return title.to_owned();
                    }
                }
            }
            return first_line.to_owned();
        }
        let title: String = scene_text.chars().take(30).collect();
        if title.len() < scene_text.len() {
            format!("场景{index}: {title}...")
        } else {
            format!("场景{index}: {title}")
        }
    }

    // ── Location detection ──

    fn detect_location(scene_text: &str) -> String {
        let text = scene_text.to_lowercase();
        if text.contains("外景")
            || text.contains("ext.")
            || text.contains("室外")
            || text.contains("街道")
            || text.contains("城市")
            || text.contains("天空")
            || text.contains("森林")
            || text.contains("海边")
            || text.contains("山")
        {
            "外景".to_owned()
        } else if text.contains("内景")
            || text.contains("int.")
            || text.contains("室内")
            || text.contains("房间")
            || text.contains("客厅")
            || text.contains("卧室")
            || text.contains("办公室")
            || text.contains("咖啡馆")
            || text.contains("餐厅")
        {
            "内景".to_owned()
        } else {
            "未指定".to_owned()
        }
    }

    // ── Shot type inference ──

    fn infer_shot_type(text: &str) -> String {
        if text.contains("全景")
            || text.contains("城市")
            || text.contains("风景")
            || text.contains("俯瞰")
            || text.contains("远景")
            || text.contains("天空")
            || text.contains("skyline")
            || text.contains("landscape")
        {
            return "wide".to_owned();
        }
        if text.contains("特写")
            || text.contains("细节")
            || text.contains("表情")
            || text.contains("手指")
            || text.contains("眼睛")
            || text.contains("泪")
            || text.contains("close")
            || text.contains("detail")
        {
            return "close-up".to_owned();
        }
        if text.contains("人物")
            || text.contains("对话")
            || text.contains("交谈")
            || text.contains("面对")
            || text.contains("两人")
            || text.contains("说话")
        {
            return "medium".to_owned();
        }
        "medium".to_owned()
    }

    // ── Camera motion inference ──

    fn infer_camera_motion(text: &str) -> String {
        if text.contains("推进") || text.contains("靠近") || text.contains("走近") {
            return "dolly-in".to_owned();
        }
        if text.contains("远离") || text.contains("退后") || text.contains("拉远") {
            return "dolly-out".to_owned();
        }
        if text.contains("环绕") || text.contains("旋转") {
            return "orbit".to_owned();
        }
        if text.contains("跟随") || text.contains("追") {
            return "tracking".to_owned();
        }
        if text.contains("俯冲") || text.contains("下降") {
            return "crane-down".to_owned();
        }
        if text.contains("升起") || text.contains("上升") || text.contains("仰望") {
            return "crane-up".to_owned();
        }
        if text.contains("摇") || text.contains("扫过") || text.contains("环顾") {
            return "pan".to_owned();
        }
        "static".to_owned()
    }

    // ── Visual intent generation ──

    fn generate_visual_intent(description: &str, style: &str, shot_type: &str) -> String {
        let style_prefix = if style.is_empty() {
            String::new()
        } else {
            format!("{}, ", style)
        };

        let translations = &[
            ("夜晚", "night"),
            ("白天", "daytime"),
            ("黄昏", "dusk"),
            ("黎明", "dawn"),
            ("雨天", "rainy"),
            ("雪天", "snowy"),
            ("城市", "city"),
            ("森林", "forest"),
            ("海边", "seaside"),
            ("室内", "indoor"),
            ("街道", "street"),
            ("霓虹灯", "neon lights"),
            ("阳光", "sunlight"),
            ("月光", "moonlight"),
        ];

        let mut english_hints: Vec<&str> = Vec::new();
        for (zh, en) in translations {
            if description.contains(zh) {
                english_hints.push(en);
            }
        }

        let shot_label = match shot_type {
            "wide" => "wide establishing shot, cinematic lighting",
            "close-up" => "close-up shot, shallow depth of field",
            _ => "medium shot, balanced composition",
        };

        if english_hints.is_empty() {
            format!("{style_prefix}{description}, {shot_label}")
        } else {
            format!(
                "{style_prefix}{hints}, {description}, {shot_label}",
                hints = english_hints.join(", "),
            )
        }
    }

    // ── Title extraction ──

    fn extract_suggested_title(text: &str) -> String {
        let first_line = text.lines().next().unwrap_or("").trim();
        if !first_line.is_empty() && first_line.len() <= 50 {
            return first_line.to_owned();
        }
        let title: String = text.chars().take(20).collect();
        if title.len() < text.len() {
            format!("{title}...")
        } else {
            title
        }
    }

    // ── Character reference extraction ──

    /// Extract character names from shot text (basic heuristic).
    fn extract_character_refs(text: &str) -> Vec<String> {
        let mut refs = Vec::new();

        // Look for quoted names: "小明" 「小明」
        for (open, close) in &[('"', '"'), ('“', '”'), ('「', '」'), ('『', '』')] {
            let mut in_quote = false;
            let mut start = 0;
            for (i, ch) in text.char_indices() {
                // 对称引号（如 "）的闭引号会再次命中 open 分支，必须先判 close。
                if ch == *close && in_quote {
                    in_quote = false;
                    let name = text[start..i].trim();
                    if !name.is_empty() && name.len() <= 20 {
                        refs.push(name.to_owned());
                    }
                } else if ch == *open {
                    in_quote = true;
                    start = i + ch.len_utf8();
                }
            }
        }

        // Look for "角色: XXX" or "角色名：XXX" patterns.
        // 中文无空格边界：遇到常见动作/判断字或超过 4 字即停止，只取人名。
        const NAME_STOP: &[char] = &[
            '走', '来', '去', '说', '看', '坐', '站', '拿', '推', '拉', '跑', '跳', '笑', '喊',
            '问', '答', '想', '吃', '喝', '开', '关', '进', '出', '回', '转', '接', '放', '写',
            '读', '听', '打', '举', '抱', '跟', '在', '是', '了', '的',
        ];
        for marker in &["角色名：", "角色：", "角色:"] {
            if let Some(pos) = text.find(marker) {
                let after = &text[pos + marker.len()..];
                let name: String = after
                    .chars()
                    .take_while(|c| {
                        !c.is_whitespace() && *c != '，' && *c != ',' && !NAME_STOP.contains(c)
                    })
                    .take(4)
                    .collect();
                if !name.is_empty() && name.len() <= 20 {
                    refs.push(name);
                }
            }
        }

        refs.sort();
        refs.dedup();
        refs
    }
}

// ──────────────────────────────────────────────────────────────────
// Tests
// ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_basic_script() {
        let service = ScriptParserService;
        let script = "场景一：城市街道\n夜晚的城市街道，霓虹灯闪烁。一个身影走过。\n\n场景二：室内\n房间内，灯光昏暗。两人对话。";
        let plan = service.parse(script, None, None).unwrap();
        assert!(plan.scenes.len() >= 2);
        assert!(plan.total_shots >= 2);
    }

    #[test]
    fn parse_with_style() {
        let service = ScriptParserService;
        let script = "场景：海边\n黄昏的海边，阳光温暖。一个人在沙滩上行走。";
        let plan = service.parse(script, Some("anime style"), None).unwrap();
        assert_eq!(plan.style.as_deref(), Some("anime style"));
        assert!(plan.scenes[0].shots[0]
            .visual_intent
            .as_ref()
            .unwrap()
            .contains("anime style"));
    }

    #[test]
    fn parse_empty_rejected() {
        let service = ScriptParserService;
        assert!(matches!(
            service.parse("", None, None),
            Err(ScriptPlanError::EmptyScript)
        ));
    }

    #[test]
    fn shot_type_inference() {
        assert_eq!(
            ScriptParserService::infer_shot_type("全景城市天际线"),
            "wide"
        );
        assert_eq!(
            ScriptParserService::infer_shot_type("特写她的表情"),
            "close-up"
        );
        assert_eq!(ScriptParserService::infer_shot_type("两人在对话"), "medium");
        assert_eq!(ScriptParserService::infer_shot_type("普通描述"), "medium");
    }

    #[test]
    fn camera_motion_inference() {
        assert_eq!(
            ScriptParserService::infer_camera_motion("镜头推进"),
            "dolly-in"
        );
        assert_eq!(
            ScriptParserService::infer_camera_motion("跟随主角"),
            "tracking"
        );
        assert_eq!(
            ScriptParserService::infer_camera_motion("环绕拍摄"),
            "orbit"
        );
        assert_eq!(
            ScriptParserService::infer_camera_motion("普通场景"),
            "static"
        );
    }

    #[test]
    fn character_ref_extraction() {
        let refs = ScriptParserService::extract_character_refs("\"小明\"看着「小红」说：你好");
        assert!(refs.contains(&"小明".to_owned()));
        assert!(refs.contains(&"小红".to_owned()));
    }

    #[test]
    fn character_ref_from_marker() {
        let refs = ScriptParserService::extract_character_refs("角色：张三走进房间");
        assert!(refs.contains(&"张三".to_owned()));
    }

    #[test]
    fn visual_intent_contains_english_hints() {
        let visual_intent =
            ScriptParserService::generate_visual_intent("夜晚的城市街道", "", "wide");
        assert!(visual_intent.contains("night"));
        assert!(visual_intent.contains("city"));
        assert!(visual_intent.contains("wide establishing shot"));
    }

    #[test]
    fn serialization_roundtrip() {
        let service = ScriptParserService;
        let plan = service
            .parse("场景一\n一个镜头。", Some("cinematic"), None)
            .unwrap();
        let json = serde_json::to_string(&plan).unwrap();
        let back: ScriptPlan = serde_json::from_str(&json).unwrap();
        assert_eq!(back.scenes.len(), plan.scenes.len());
        assert_eq!(back.total_shots, plan.total_shots);
    }
}
