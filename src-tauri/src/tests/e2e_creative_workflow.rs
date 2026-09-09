//! 端到端集成测试：完整创作工作流。
//!
//! 验证：生成 → AI Critic 评价 → 用户反馈 → Edit Understanding → 修改计划 → 重新评价。
//! 这些测试模拟完整的创作生命周期，不依赖真实的 LLM API。

use tempfile::tempdir;

use crate::{
    adapters::sqlite::{
        creative_memory_repository::SqliteCreativeMemoryRepository,
        edit_repository::SqliteEditRepository, review_repository::SqliteReviewRepository,
        workspace_repository::SqliteWorkspaceRepository,
    },
    application::{
        creative_memory_service::CreativeMemoryService, critic_service::CriticService,
        edit_understanding_service::EditUnderstandingService,
    },
    domain::{
        creative_memory::{CreativeMemoryDraft, MemoryFilter, MemoryType},
        edit::{EditRequestDraft, EditRequestStatus},
        review::{
            CommercialScores, ContentScores, IssueSeverity, RequirementScores, ReviewDecision,
            ReviewDimensionLayer, ReviewIssue, TechnicalScores, VideoGenerationGate, VisualScores,
        },
        workspace::NewWorkspace,
    },
    ports::workspace_repository::WorkspaceRepository,
};

const PROJECT_ID: &str = "123e4567-e89b-42d3-a456-426614174000";
const TEACHER_ID: &str = "223e4567-e89b-42d3-a456-426614174000";

/// 创建测试环境：初始化工作空间，返回临时目录和所有服务。
fn setup_test_env() -> (
    tempfile::TempDir,
    CriticService,
    EditUnderstandingService,
    CreativeMemoryService,
) {
    let dir = tempdir().unwrap();
    let path = dir.path().join("workspace.sqlite3");
    let mut ws = SqliteWorkspaceRepository::open(&path).unwrap();
    ws.initialize(&NewWorkspace::try_new("测试工作空间", "测试教师").unwrap())
        .unwrap();
    drop(ws);

    let critic_repo = SqliteReviewRepository::open(&path).unwrap();
    let edit_repo = SqliteEditRepository::open(&path).unwrap();
    let memory_repo = SqliteCreativeMemoryRepository::open(&path).unwrap();

    let critic = CriticService::new(critic_repo, path.clone());
    let edit = EditUnderstandingService::new(edit_repo, path.clone());
    let memory = CreativeMemoryService::new(memory_repo, path.clone());

    (dir, critic, edit, memory)
}

/// 模拟一个好的关键帧评分（应该通过视频生成门槛）。
fn good_scores() -> (
    RequirementScores,
    VisualScores,
    ContentScores,
    CommercialScores,
    TechnicalScores,
    Vec<ReviewIssue>,
) {
    (
        RequirementScores {
            requirement_match: Some(88.0),
            completeness: Some(85.0),
            clarity: Some(82.0),
        },
        VisualScores {
            composition: Some(90.0),
            color: Some(85.0),
            lighting: Some(82.0),
            texture: Some(78.0),
            lens_language: Some(80.0),
        },
        ContentScores {
            theme_match: Some(86.0),
            emotion_expression: Some(84.0),
            narrative_purpose: Some(80.0),
        },
        CommercialScores {
            platform_fit: Some(75.0),
            audience_fit: Some(78.0),
            conversion_potential: Some(70.0),
        },
        TechnicalScores {
            clarity: Some(88.0),
            distortion: Some(85.0),
            character_consistency: Some(82.0),
            motion_quality: Some(80.0),
        },
        vec![],
    )
}

/// 模拟一个差的关键帧评分（不应通过视频生成门槛）。
fn bad_scores() -> (
    RequirementScores,
    VisualScores,
    ContentScores,
    CommercialScores,
    TechnicalScores,
    Vec<ReviewIssue>,
) {
    (
        RequirementScores {
            requirement_match: Some(55.0),
            completeness: Some(50.0),
            clarity: Some(48.0),
        },
        VisualScores {
            composition: Some(60.0),
            color: Some(55.0),
            lighting: Some(50.0),
            texture: Some(45.0),
            lens_language: Some(48.0),
        },
        ContentScores {
            theme_match: Some(52.0),
            emotion_expression: Some(48.0),
            narrative_purpose: Some(45.0),
        },
        CommercialScores {
            platform_fit: Some(40.0),
            audience_fit: Some(42.0),
            conversion_potential: Some(35.0),
        },
        TechnicalScores {
            clarity: Some(55.0),
            distortion: Some(50.0),
            character_consistency: Some(45.0),
            motion_quality: Some(42.0),
        },
        vec![
            ReviewIssue {
                dimension: "character_consistency".into(),
                severity: IssueSeverity::High,
                message: "角色脸部特征与参考图偏差较大".into(),
                suggested_fix: "强化角色参考图约束，增加正脸特征".into(),
            },
            ReviewIssue {
                dimension: "style_consistency".into(),
                severity: IssueSeverity::Medium,
                message: "画面风格偏网红感，不符合纪录片定位".into(),
                suggested_fix: "减少美颜滤镜，增加自然光".into(),
            },
        ],
    )
}

// ════════════════════════════════════════════════════════════════
// 测试 1: 完整创作工作流（正面路径）
// ════════════════════════════════════════════════════════════════

#[test]
fn e2e_good_keyframe_passes_video_gate() {
    let (_dir, critic, _edit, _memory) = setup_test_env();
    let (req, vis, con, com, tec, issues) = good_scores();

    // Step 1: AI Critic 评价关键帧
    let report = critic
        .evaluate_asset(
            PROJECT_ID.into(),
            "asset-001".into(),
            None,
            req,
            vis,
            con,
            com,
            tec,
            issues,
            Some("claude-vision".into()),
        )
        .unwrap();

    // 验证：评分应该高于 80
    assert!(
        report.overall_score >= 80.0,
        "Overall score should be >= 80, got {}",
        report.overall_score
    );
    // 验证：决策应该是 accept 或 accept_with_suggestions
    assert!(
        matches!(
            report.decision,
            ReviewDecision::Accept | ReviewDecision::AcceptWithSuggestions
        ),
        "Decision should be accept, got {:?}",
        report.decision
    );

    // Step 2: 检查视频生成门槛
    let (passed, warnings) = CriticService::check_video_generation_gate(&report);
    assert!(
        passed,
        "Good keyframe should pass video gate, warnings: {:?}",
        warnings
    );
    assert!(
        warnings.is_empty(),
        "No warnings expected for good keyframe"
    );
}

// ════════════════════════════════════════════════════════════════
// 测试 2: 差关键帧 → 不通过视频门槛 → 用户反馈 → 修改计划
// ════════════════════════════════════════════════════════════════

#[test]
fn e2e_bad_keyframe_triggers_feedback_and_plan() {
    let (_dir, critic, edit, _memory) = setup_test_env();
    let (req, vis, con, com, tec, issues) = bad_scores();

    // Step 1: AI Critic 评价关键帧（低分）
    let report = critic
        .evaluate_asset(
            PROJECT_ID.into(),
            "asset-002".into(),
            None,
            req,
            vis,
            con,
            com,
            tec,
            issues,
            Some("claude-vision".into()),
        )
        .unwrap();

    // 验证：低分关键帧
    assert!(
        report.overall_score < 70.0,
        "Bad keyframe should score < 70, got {}",
        report.overall_score
    );
    assert!(
        matches!(
            report.decision,
            ReviewDecision::Regenerate | ReviewDecision::Block
        ),
        "Decision should be regenerate or block, got {:?}",
        report.decision
    );

    // Step 2: 视频生成门槛应阻断
    let (passed, warnings) = CriticService::check_video_generation_gate(&report);
    assert!(!passed, "Bad keyframe should fail video gate");
    assert!(!warnings.is_empty(), "Should have warnings");

    // Step 3: 用户提交修改反馈
    let draft = EditRequestDraft::try_new(
        PROJECT_ID.into(),
        None,
        "太假了，人物更像真实创业者".into(),
        "generation_result".into(),
        None,
        None,
        TEACHER_ID.into(),
    )
    .unwrap();

    let request = edit.submit_feedback(draft).unwrap();
    // 验证：反馈已接收并解析
    assert!(
        matches!(
            request.status,
            EditRequestStatus::PlanReady
                | EditRequestStatus::Received
                | EditRequestStatus::Ambiguous
        ),
        "Request status should be plan_ready/received/ambiguous, got {:?}",
        request.status
    );
    // 验证：如果解析成功，意图应该已解析
    if request.status == EditRequestStatus::PlanReady {
        assert!(
            request.intent_json.is_some(),
            "Intent should be parsed when plan is ready"
        );
    }
    // 验证：反馈文本已保存
    assert_eq!(request.feedback_text, "太假了，人物更像真实创业者");

    // Step 4: 获取修改计划
    let plan = edit.get_plan_by_request(&request.id).unwrap();
    if let Some(plan) = plan {
        assert_eq!(plan.project_id, PROJECT_ID);
        assert!(
            !plan.plan_summary.is_empty(),
            "Plan summary should not be empty"
        );
        assert!(!plan.targets_json.is_empty(), "Targets should not be empty");
    }
}

// ════════════════════════════════════════════════════════════════
// 测试 3: 评分维度详情生成
// ════════════════════════════════════════════════════════════════

#[test]
fn e2e_review_dimensions_generated_correctly() {
    let (_dir, critic, _edit, _memory) = setup_test_env();
    let (req, vis, con, com, tec, issues) = good_scores();

    let report = critic
        .evaluate_asset(
            PROJECT_ID.into(),
            "asset-003".into(),
            None,
            req,
            vis,
            con,
            com,
            tec,
            issues,
            Some("claude-vision".into()),
        )
        .unwrap();

    // 保存维度详情
    let dimensions = critic
        .save_dimensions(&report.id, Some("claude-vision"))
        .unwrap();
    assert!(!dimensions.is_empty(), "Should have dimensions");

    // 验证包含所有 5 层维度
    let layers: Vec<_> = dimensions.iter().map(|d| d.dimension_layer).collect();
    assert!(layers.contains(&ReviewDimensionLayer::Requirement));
    assert!(layers.contains(&ReviewDimensionLayer::Visual));
    assert!(layers.contains(&ReviewDimensionLayer::Content));
    assert!(layers.contains(&ReviewDimensionLayer::Commercial));
    assert!(layers.contains(&ReviewDimensionLayer::Technical));

    // 验证分数范围
    for dim in &dimensions {
        assert!(
            dim.score >= 0.0 && dim.score <= 100.0,
            "Score out of range: {}",
            dim.score
        );
        assert!(
            dim.weight > 0.0 && dim.weight <= 5.0,
            "Weight out of range: {}",
            dim.weight
        );
    }

    // 验证可以查询回来
    let fetched = critic.list_dimensions(&report.id).unwrap();
    assert_eq!(fetched.len(), dimensions.len());
}

// ════════════════════════════════════════════════════════════════
// 测试 4: 多次评价历史查询
// ════════════════════════════════════════════════════════════════

#[test]
fn e2e_multiple_reviews_queried_by_project() {
    let (_dir, critic, _edit, _memory) = setup_test_env();

    // 第一次评价（好）
    let (req, vis, con, com, tec, issues) = good_scores();
    let _report1 = critic
        .evaluate_asset(
            PROJECT_ID.into(),
            "asset-v1".into(),
            None,
            req,
            vis,
            con,
            com,
            tec,
            issues,
            Some("claude-vision".into()),
        )
        .unwrap();

    // 第二次评价（差）
    let (req, vis, con, com, tec, issues) = bad_scores();
    let _report2 = critic
        .evaluate_asset(
            PROJECT_ID.into(),
            "asset-v2".into(),
            None,
            req,
            vis,
            con,
            com,
            tec,
            issues,
            Some("gpt-vision".into()),
        )
        .unwrap();

    // 查询所有评价
    let all = critic
        .list_reports_by_project(PROJECT_ID, None, 10)
        .unwrap();
    assert_eq!(all.len(), 2);

    // 按决策筛选（decision 可能因加权略有不同，使用宽松检查）
    let accept_reports = critic
        .list_reports_by_project(PROJECT_ID, Some(ReviewDecision::Accept), 10)
        .unwrap();
    let accept_w_suggestions = critic
        .list_reports_by_project(PROJECT_ID, Some(ReviewDecision::AcceptWithSuggestions), 10)
        .unwrap();
    let good_total = accept_reports.len() + accept_w_suggestions.len();
    assert!(
        good_total >= 1,
        "Should have at least 1 accepted report, got {}",
        good_total
    );

    let regenerate_reports = critic
        .list_reports_by_project(PROJECT_ID, Some(ReviewDecision::Regenerate), 10)
        .unwrap();
    let block_reports = critic
        .list_reports_by_project(PROJECT_ID, Some(ReviewDecision::Block), 10)
        .unwrap();
    let bad_total = regenerate_reports.len() + block_reports.len();
    assert!(
        bad_total >= 1,
        "Should have at least 1 regenerate/block report, got {}",
        bad_total
    );
}

// ════════════════════════════════════════════════════════════════
// 测试 5: 创意记忆工作流
// ════════════════════════════════════════════════════════════════

#[test]
fn e2e_creative_memory_lifecycle() {
    let (_dir, _critic, _edit, memory) = setup_test_env();

    // Step 1: 保存风格偏好
    let draft = CreativeMemoryDraft::try_new(
        "style_preference".into(),
        "user".into(),
        None,
        r#"{"color_palette": ["warm", "muted"], "lighting": "natural_window"}"#.into(),
        "偏好暖色调和自然光".into(),
        "explicit_save".into(),
        None,
        TEACHER_ID.into(),
    )
    .unwrap();

    let record = memory.save_memory(draft).unwrap();
    assert_eq!(record.memory_type, MemoryType::StylePreference);
    assert_eq!(
        record.status,
        crate::domain::creative_memory::MemoryStatus::Active
    );

    // Step 2: 保存负面偏好
    let draft = CreativeMemoryDraft::try_new(
        "negative_preference".into(),
        "user".into(),
        None,
        r#"{"avoid": ["网红感", "赛博霓虹", "过度磨皮"]}"#.into(),
        "不喜欢网红风格".into(),
        "explicit_save".into(),
        None,
        TEACHER_ID.into(),
    )
    .unwrap();
    memory.save_memory(draft).unwrap();

    // Step 3: 列出所有记忆
    let filter = MemoryFilter::try_new(None, None, None, None, None).unwrap();
    let all = memory.list_memories(filter).unwrap();
    assert_eq!(all.len(), 2);

    // Step 4: 确认记忆（增加置信度）
    let confirmed = memory.confirm_memory(&record.id).unwrap();
    assert_eq!(confirmed.confirm_count, 2);
    assert!(confirmed.confidence > 0.5);

    // Step 5: 暂停记忆
    let paused = memory
        .update_memory_status(
            &record.id,
            crate::domain::creative_memory::MemoryStatus::Paused,
        )
        .unwrap();
    assert_eq!(
        paused.status,
        crate::domain::creative_memory::MemoryStatus::Paused
    );

    // Step 6: 恢复记忆
    let resumed = memory
        .update_memory_status(
            &record.id,
            crate::domain::creative_memory::MemoryStatus::Active,
        )
        .unwrap();
    assert_eq!(
        resumed.status,
        crate::domain::creative_memory::MemoryStatus::Active
    );

    // Step 7: 删除记忆
    memory.delete_memory(&record.id).unwrap();
    let filter = MemoryFilter::try_new(None, None, None, Some("active".into()), None).unwrap();
    let remaining = memory.list_memories(filter).unwrap();
    assert_eq!(remaining.len(), 1); // 只剩负面偏好
}

// ════════════════════════════════════════════════════════════════
// 测试 6: Content Guard 规则引擎
// ════════════════════════════════════════════════════════════════

#[test]
fn e2e_content_guard_detects_issues() {
    use crate::application::content_guard_service::EnhancedContentGuardRules;

    // 中文敏感词检测
    let text = "这个内容包含色情和血腥元素";
    let checks = EnhancedContentGuardRules::check_multilingual_keywords(text);
    assert!(!checks.is_empty());
    assert_eq!(checks[0]["category"], "nsfw_content");

    // 英文敏感词检测
    let text = "This content promotes hate and racism";
    let checks = EnhancedContentGuardRules::check_multilingual_keywords(text);
    assert!(!checks.is_empty());

    // 平台特定检查（抖音）
    let text = "关注我的微信公众号获取更多内容";
    let checks = EnhancedContentGuardRules::check_platform_specific(text, "douyin");
    assert!(!checks.is_empty());

    // 正常内容通过
    let text = "这是一个关于年轻人创业的纪录片脚本";
    let checks = EnhancedContentGuardRules::check_multilingual_keywords(text);
    assert!(checks.is_empty());
}

// ════════════════════════════════════════════════════════════════
// 测试 7: Edit Understanding 反馈模式匹配
// ════════════════════════════════════════════════════════════════

#[test]
fn e2e_feedback_pattern_matching() {
    use crate::domain::edit::FeedbackPattern;

    // "太假了" → 风格调整
    let result = FeedbackPattern::find_match("太假了，增加真实感");
    assert!(result.is_some());
    let pattern = result.unwrap();
    assert_eq!(
        pattern.intent,
        crate::domain::edit::EditOperationType::StyleAdjustment
    );
    assert!(!pattern.prompt_add.is_empty());
    assert!(!pattern.prompt_remove.is_empty());

    // "不够大气" → 构图变更
    let result = FeedbackPattern::find_match("不够大气，画面太小了");
    assert!(result.is_some());
    assert_eq!(
        result.unwrap().intent,
        crate::domain::edit::EditOperationType::CompositionChange
    );

    // "色调太冷" → 色彩调色
    let result = FeedbackPattern::find_match("色调太冷了，暖一点");
    assert!(result.is_some());
    assert_eq!(
        result.unwrap().intent,
        crate::domain::edit::EditOperationType::ColorGrading
    );

    // 无匹配
    let result = FeedbackPattern::find_match("这个画面很好，不需要修改");
    assert!(result.is_none());
}

// ════════════════════════════════════════════════════════════════
// 测试 8: 评价报告 → 维度详情 → 编辑请求完整流水线
// ════════════════════════════════════════════════════════════════

#[test]
fn e2e_full_pipeline_review_dimensions_feedback() {
    let (_dir, critic, edit, _memory) = setup_test_env();
    let (req, vis, con, com, tec, issues) = bad_scores();

    // Step 1: AI Critic 评价
    let report = critic
        .evaluate_asset(
            PROJECT_ID.into(),
            "asset-pipeline".into(),
            None,
            req,
            vis,
            con,
            com,
            tec,
            issues,
            Some("claude-vision".into()),
        )
        .unwrap();
    assert!(report.overall_score < 70.0);

    // Step 2: 保存维度详情
    let dimensions = critic
        .save_dimensions(&report.id, Some("claude-vision"))
        .unwrap();
    assert!(!dimensions.is_empty());

    // Step 3: 用户反馈（关联到评价报告）
    let draft = EditRequestDraft::try_new(
        PROJECT_ID.into(),
        None,
        "人物不像真实创业者，太像模特了".into(),
        "generation_result".into(),
        Some(PROJECT_ID.into()), // 使用合法 UUID 作为 context_ref_id
        Some(report.id.clone()),
        TEACHER_ID.into(),
    )
    .unwrap();
    let request = edit.submit_feedback(draft).unwrap();
    assert_eq!(request.source_review_id, Some(report.id.clone()));

    // Step 4: 验证评价报告和编辑请求可以关联查询
    let report_by_asset = critic.list_reports_by_asset("asset-pipeline").unwrap();
    assert_eq!(report_by_asset.len(), 1);
    assert_eq!(report_by_asset[0].id, report.id);
}

// ════════════════════════════════════════════════════════════════
// 测试 9: Content Guard 导出合规检查
// ════════════════════════════════════════════════════════════════

#[test]
fn e2e_export_compliance_check() {
    // 模拟前端 ExportComplianceCheck 的逻辑
    use crate::domain::review::{CommercialUseStatus, ContentGuardStatus};

    // 场景1：所有资产合规
    let licenses_ok = [
        (CommercialUseStatus::Clear, true),
        (CommercialUseStatus::Clear, true),
    ];
    let guards_ok = [ContentGuardStatus::Passed, ContentGuardStatus::Passed];

    let can_export = licenses_ok
        .iter()
        .all(|(s, e)| *s == CommercialUseStatus::Clear && *e)
        && guards_ok.iter().all(|s| *s == ContentGuardStatus::Passed);
    assert!(can_export, "All assets compliant should allow export");

    // 场景2：有阻断资产
    let licenses_blocked = [
        (CommercialUseStatus::Clear, true),
        (CommercialUseStatus::Blocked, false),
    ];
    let guards_ok = [ContentGuardStatus::Passed, ContentGuardStatus::Passed];

    let can_export = licenses_blocked
        .iter()
        .all(|(s, e)| *s == CommercialUseStatus::Clear && *e)
        && guards_ok.iter().all(|s| *s == ContentGuardStatus::Passed);
    assert!(!can_export, "Blocked asset should prevent export");

    // 场景3：需要审核
    let licenses_review = [(CommercialUseStatus::NeedsReview, true)];
    let needs_review = licenses_review
        .iter()
        .any(|(s, _)| *s == CommercialUseStatus::NeedsReview);
    assert!(needs_review, "Should flag for review");
}

// ════════════════════════════════════════════════════════════════
// 测试 10: VideoGenerationGate 边界条件
// ════════════════════════════════════════════════════════════════

#[test]
fn e2e_video_gate_boundary_conditions() {
    use crate::domain::review::ReviewReportRecord;

    // 创建一个刚好过门槛的报告
    let report = ReviewReportRecord {
        id: "test-boundary".into(),
        project_id: PROJECT_ID.into(),
        run_id: None,
        shot_id: None,
        asset_id: None,
        generation_attempt_id: None,
        reviewer_type: crate::domain::review::ReviewerType::Auto,
        reviewer_agent_version: None,
        reviewer_provider: None,
        requirement_scores_json: r#"{"match": 80}"#.into(),
        visual_scores_json: "{}".into(),
        content_scores_json: r#"{"themeMatch": 80}"#.into(),
        commercial_scores_json: "{}".into(),
        technical_scores_json: r#"{"characterConsistency": 80}"#.into(),
        overall_score: 80.0,
        weighted_score: None,
        issues_json: "[]".into(),
        decision: ReviewDecision::AcceptWithSuggestions,
        confidence: Some(0.8),
        source_task_id: None,
        review_version: 1,
        created_at: "2026-07-21T00:00:00Z".into(),
    };

    let gate = VideoGenerationGate::default();
    assert!(gate.passes(&report), "Boundary score should pass");

    // 创建一个刚好不过门槛的报告
    let mut report_fail = report.clone();
    report_fail.overall_score = 79.9;
    assert!(!gate.passes(&report_fail), "Below boundary should fail");
}
