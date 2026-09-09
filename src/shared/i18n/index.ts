/**
 * i18n — 国际化翻译管理系统。
 *
 * 提供统一的翻译管理机制，支持：
 * - 按模块组织翻译键
 * - 类型安全的翻译访问
 * - 动态参数替换
 * - 回退语言支持
 */

// ═══════════════════════════════════════════════════
// 翻译键类型定义
// ═══════════════════════════════════════════════════

export type Locale = "zh-CN" | "en-US";

export interface TranslationKeys {
  // 通用
  common: {
    confirm: string;
    cancel: string;
    save: string;
    delete: string;
    edit: string;
    create: string;
    loading: string;
    error: string;
    success: string;
    warning: string;
    info: string;
    yes: string;
    no: string;
    ok: string;
    back: string;
    next: string;
    previous: string;
    search: string;
    filter: string;
    sort: string;
    refresh: string;
    close: string;
    open: string;
    select: string;
    clear: string;
    reset: string;
    submit: string;
    retry: string;
    cancel_operation: string;
    confirm_operation: string;
    operation_success: string;
    operation_failed: string;
    no_data: string;
    no_results: string;
    loading_data: string;
    saving: string;
    deleting: string;
    creating: string;
    updating: string;
  };

  // 认证
  auth: {
    login: string;
    logout: string;
    register: string;
    username: string;
    password: string;
    email: string;
    forgot_password: string;
    reset_password: string;
    login_success: string;
    login_failed: string;
    logout_success: string;
    session_expired: string;
    invalid_credentials: string;
    account_locked: string;
    account_disabled: string;
    password_changed: string;
    password_mismatch: string;
    username_taken: string;
    email_taken: string;
  };

  // 工作空间
  workspace: {
    title: string;
    initialize: string;
    initialized: string;
    not_initialized: string;
    name: string;
    teacher_name: string;
    database_status: string;
    storage_status: string;
    backup_status: string;
    schema_version: string;
    sqlite_version: string;
    journal_mode: string;
    foreign_keys: string;
    asset_count: string;
    total_bytes: string;
    missing_assets: string;
    directories_ready: string;
    writable: string;
    manifest_ready: string;
  };

  // 教学管理
  teaching: {
    classrooms: string;
    students: string;
    teachers: string;
    classroom: string;
    student: string;
    teacher: string;
    create_classroom: string;
    edit_classroom: string;
    delete_classroom: string;
    classroom_name: string;
    classroom_code: string;
    classroom_description: string;
    teaching_goal: string;
    start_date: string;
    end_date: string;
    status: string;
    planned: string;
    active: string;
    completed: string;
    archived: string;
    student_count: string;
    add_student: string;
    import_students: string;
    export_students: string;
    student_name: string;
    student_id: string;
    student_email: string;
    student_phone: string;
    student_gender: string;
    student_notes: string;
    male: string;
    female: string;
    unknown: string;
    roster_number: string;
    join_date: string;
    withdraw_student: string;
    transfer_student: string;
    batch_operations: string;
    select_all: string;
    deselect_all: string;
    selected_count: string;
    no_students: string;
    no_classrooms: string;
    search_placeholder: string;
    filter_by_classroom: string;
    filter_by_status: string;
  };

  // 生成任务
  generation: {
    title: string;
    submit_task: string;
    task_list: string;
    task_details: string;
    provider: string;
    model: string;
    prompt: string;
    negative_prompt: string;
    parameters: string;
    status: string;
    pending: string;
    running: string;
    succeeded: string;
    failed: string;
    cancelled: string;
    timed_out: string;
    progress: string;
    started_at: string;
    completed_at: string;
    error_message: string;
    retry: string;
    cancel: string;
    view_result: string;
    download_result: string;
    no_tasks: string;
    task_history: string;
    clear_history: string;
    export_history: string;
  };

  // 资产管理
  assets: {
    title: string;
    import_assets: string;
    export_assets: string;
    asset_list: string;
    asset_details: string;
    asset_name: string;
    asset_type: string;
    asset_size: string;
    asset_hash: string;
    asset_status: string;
    valid: string;
    invalid: string;
    missing: string;
    corrupted: string;
    quarantined: string;
    storage_namespace: string;
    workspace: string;
    generation: string;
    teaching_resource: string;
    system: string;
    integrity_status: string;
    metadata: string;
    created_at: string;
    updated_at: string;
    no_assets: string;
    search_assets: string;
    filter_by_type: string;
    filter_by_status: string;
    reverify_integrity: string;
    open_containing_folder: string;
    delete_asset: string;
    batch_delete: string;
    import_progress: string;
    import_success: string;
    import_failed: string;
    skipped_duplicates: string;
    skipped_errors: string;
  };

  // AI 创作
  creative: {
    title: string;
    creative_workshop: string;
    new_project: string;
    project_name: string;
    project_description: string;
    project_status: string;
    draft: string;
    in_progress: string;
    review: string;
    completed: string;
    archived: string;
    creative_run: string;
    user_goal: string;
    workflow_type: string;
    current_stage: string;
    requirement_analysis: string;
    visual_spec: string;
    story_planning: string;
    character_planning: string;
    human_approval: string;
    image_generation: string;
    video_generation: string;
    review_stage: string;
    optimization: string;
    editing: string;
    final_review: string;
    export_stage: string;
    budget_limit: string;
    cost_estimate: string;
    scenes: string;
    shots: string;
    characters: string;
    scripts: string;
    storyboards: string;
    prompts: string;
    assets_generated: string;
    no_projects: string;
    no_scenes: string;
    no_shots: string;
    no_characters: string;
    create_scene: string;
    create_shot: string;
    create_character: string;
    scene_name: string;
    shot_description: string;
    shot_duration: string;
    shot_camera: string;
    shot_movement: string;
    shot_emotion: string;
    shot_purpose: string;
    shot_voiceover: string;
    shot_image_prompt: string;
    shot_video_prompt: string;
    character_name: string;
    character_role: string;
    character_profile: string;
    character_appearance: string;
    character_clothing: string;
    character_style_lock: string;
  };

  // AI 评价
  critic: {
    title: string;
    review_report: string;
    overall_score: string;
    decision: string;
    needs_review: string;
    accept: string;
    accept_with_suggestions: string;
    revise: string;
    regenerate: string;
    block: string;
    requirement_layer: string;
    visual_layer: string;
    content_layer: string;
    commercial_layer: string;
    technical_layer: string;
    match: string;
    completeness: string;
    clarity: string;
    composition: string;
    color: string;
    lighting: string;
    texture: string;
    lens_language: string;
    theme_match: string;
    emotion_expression: string;
    narrative_purpose: string;
    platform_fit: string;
    audience_fit: string;
    conversion_potential: string;
    distortion: string;
    character_consistency: string;
    motion_quality: string;
    issues_found: string;
    no_issues: string;
    suggestions: string;
    confidence: string;
    reviewer_type: string;
    auto: string;
    manual: string;
    hybrid: string;
    video_generation_gate: string;
    gate_passed: string;
    gate_failed: string;
    gate_requirements: string;
    overall_min: string;
    character_consistency_min: string;
    style_consistency_min: string;
    requirement_match_min: string;
  };

  // 编辑理解
  edit: {
    title: string;
    feedback: string;
    submit_feedback: string;
    feedback_placeholder: string;
    analyzing: string;
    plan_ready: string;
    apply_plan: string;
    skip_plan: string;
    plan_summary: string;
    operation_type: string;
    style_adjustment: string;
    character_adjustment: string;
    composition_change: string;
    lighting_adjustment: string;
    color_grading: string;
    camera_change: string;
    content_revision: string;
    quality_improvement: string;
    regenerate_op: string;
    prompt_patch: string;
    add_keywords: string;
    remove_keywords: string;
    parameter_patch: string;
    model_change: string;
    steps_change: string;
    guidance_change: string;
    requires_regeneration: string;
    requires_critic_rerun: string;
    risk_level: string;
    low_risk: string;
    medium_risk: string;
    high_risk: string;
    edit_history: string;
    no_edits: string;
    edit_applied: string;
    edit_skipped: string;
    feedback_received: string;
    intent_parsed: string;
    quick_hints: string;
    smart_suggestions: string;
  };

  // 内容安全
  content_guard: {
    title: string;
    safety_check: string;
    status: string;
    passed: string;
    needs_review: string;
    flagged: string;
    blocked: string;
    risk_level: string;
    low: string;
    medium: string;
    high: string;
    critical: string;
    checks_performed: string;
    political_sensitivity: string;
    nsfw_content: string;
    hate_speech: string;
    illegal_content: string;
    minor_protection: string;
    professional_risk: string;
    brand_consistency: string;
    platform_compliance: string;
    educational_safety: string;
    allowed_actions: string;
    blocked_actions: string;
    export_allowed: string;
    export_blocked: string;
    publish_allowed: string;
    publish_blocked: string;
    manual_review_required: string;
    guard_version: string;
    guard_provider: string;
  };

  // 版权管理
  license: {
    title: string;
    copyright_status: string;
    ai_generated: string;
    user_uploaded: string;
    third_party: string;
    derived_work: string;
    commercial_use: string;
    clear: string;
    needs_review: string;
    restricted: string;
    blocked: string;
    unknown: string;
    export_allowed: string;
    export_blocked: string;
    attribution_required: string;
    copyright_statement: string;
    risk_flags: string;
    third_party_reference: string;
    person_likeness: string;
    brand_logo: string;
    copyrighted_material: string;
    music_copyright: string;
    font_license: string;
    provider_info: string;
    model_name: string;
    review_notes: string;
    no_license: string;
  };

  // 创意记忆
  memory: {
    title: string;
    creative_memory: string;
    memory_type: string;
    style_preference: string;
    negative_preference: string;
    brand_rule: string;
    workflow_habit: string;
    prompt_pattern: string;
    review_history: string;
    scope: string;
    user_scope: string;
    project_scope: string;
    brand_scope: string;
    add_memory: string;
    edit_memory: string;
    delete_memory: string;
    pause_memory: string;
    resume_memory: string;
    confirm_memory: string;
    memory_summary: string;
    memory_content: string;
    confidence: string;
    high_confidence: string;
    medium_confidence: string;
    low_confidence: string;
    active: string;
    paused: string;
    archived: string;
    no_memories: string;
    memory_created: string;
    memory_updated: string;
    memory_deleted: string;
    memory_confirmed: string;
    auto_prefill: string;
    style_prefilled: string;
    negative_prefilled: string;
    brand_prefilled: string;
  };

  // 模型路由
  model_router: {
    title: string;
    model_selection: string;
    routing_strategy: string;
    balanced: string;
    cost_optimized: string;
    quality_first: string;
    speed_first: string;
    user_specified: string;
    available_models: string;
    selected_model: string;
    model_provider: string;
    model_name: string;
    capability_score: string;
    cost_score: string;
    speed_score: string;
    quality_score: string;
    overall_score: string;
    smart_routing: string;
    route_decision: string;
    route_reason: string;
    requires_confirmation: string;
    estimated_cost: string;
    no_models: string;
    model_unavailable: string;
    provider_unavailable: string;
    quota_exceeded: string;
    rate_limited: string;
  };

  // 工作流
  workflow: {
    title: string;
    creative_workflow: string;
    start_workflow: string;
    advance_workflow: string;
    pause_workflow: string;
    resume_workflow: string;
    workflow_status: string;
    generating: string;
    evaluating: string;
    waiting_feedback: string;
    parsing_feedback: string;
    planning_modification: string;
    applying_modification: string;
    regenerating: string;
    completed: string;
    failed: string;
    iteration: string;
    max_iterations: string;
    current_stage: string;
    workflow_history: string;
    no_workflows: string;
    workflow_started: string;
    workflow_advanced: string;
    workflow_paused: string;
    workflow_resumed: string;
    workflow_completed: string;
    workflow_failed: string;
    auto_evaluation: string;
    auto_regenerate: string;
    video_gate_threshold: string;
    pause_for_feedback: string;
  };

  // 设置
  settings: {
    title: string;
    general: string;
    appearance: string;
    theme: string;
    light: string;
    dark: string;
    system: string;
    language: string;
    density: string;
    compact: string;
    comfortable: string;
    spacious: string;
    navigation: string;
    sidebar: string;
    topbar: string;
    storage: string;
    backup: string;
    restore: string;
    create_backup: string;
    restore_backup: string;
    backup_history: string;
    export_data: string;
    import_data: string;
    clear_data: string;
    about: string;
    version: string;
    build_date: string;
    license_info: string;
    check_updates: string;
    report_issue: string;
    documentation: string;
    support: string;
  };
}

// ═══════════════════════════════════════════════════
// 中文翻译
// ═══════════════════════════════════════════════════

export const zhCN: TranslationKeys = {
  common: {
    confirm: "确认",
    cancel: "取消",
    save: "保存",
    delete: "删除",
    edit: "编辑",
    create: "创建",
    loading: "加载中...",
    error: "错误",
    success: "成功",
    warning: "警告",
    info: "信息",
    yes: "是",
    no: "否",
    ok: "确定",
    back: "返回",
    next: "下一步",
    previous: "上一步",
    search: "搜索",
    filter: "筛选",
    sort: "排序",
    refresh: "刷新",
    close: "关闭",
    open: "打开",
    select: "选择",
    clear: "清除",
    reset: "重置",
    submit: "提交",
    retry: "重试",
    cancel_operation: "取消操作",
    confirm_operation: "确认操作",
    operation_success: "操作成功",
    operation_failed: "操作失败",
    no_data: "暂无数据",
    no_results: "暂无结果",
    loading_data: "加载数据中...",
    saving: "保存中...",
    deleting: "删除中...",
    creating: "创建中...",
    updating: "更新中...",
  },
  auth: {
    login: "登录",
    logout: "登出",
    register: "注册",
    username: "用户名",
    password: "密码",
    email: "邮箱",
    forgot_password: "忘记密码",
    reset_password: "重置密码",
    login_success: "登录成功",
    login_failed: "登录失败",
    logout_success: "登出成功",
    session_expired: "会话已过期",
    invalid_credentials: "用户名或密码错误",
    account_locked: "账户已锁定",
    account_disabled: "账户已禁用",
    password_changed: "密码已修改",
    password_mismatch: "密码不匹配",
    username_taken: "用户名已被使用",
    email_taken: "邮箱已被使用",
  },
  workspace: {
    title: "工作空间",
    initialize: "初始化工作空间",
    initialized: "已初始化",
    not_initialized: "未初始化",
    name: "工作空间名称",
    teacher_name: "教师姓名",
    database_status: "数据库状态",
    storage_status: "存储状态",
    backup_status: "备份状态",
    schema_version: "Schema 版本",
    sqlite_version: "SQLite 版本",
    journal_mode: "日志模式",
    foreign_keys: "外键约束",
    asset_count: "资产数量",
    total_bytes: "总字节数",
    missing_assets: "缺失资产",
    directories_ready: "目录就绪",
    writable: "可写",
    manifest_ready: "清单就绪",
  },
  teaching: {
    classrooms: "班级",
    students: "学生",
    teachers: "教师",
    classroom: "班级",
    student: "学生",
    teacher: "教师",
    create_classroom: "创建班级",
    edit_classroom: "编辑班级",
    delete_classroom: "删除班级",
    classroom_name: "班级名称",
    classroom_code: "班级代码",
    classroom_description: "班级描述",
    teaching_goal: "教学目标",
    start_date: "开始日期",
    end_date: "结束日期",
    status: "状态",
    planned: "计划中",
    active: "进行中",
    completed: "已完成",
    archived: "已归档",
    student_count: "学生人数",
    add_student: "添加学生",
    import_students: "导入学生",
    export_students: "导出学生",
    student_name: "学生姓名",
    student_id: "学号",
    student_email: "邮箱",
    student_phone: "手机号",
    student_gender: "性别",
    student_notes: "备注",
    male: "男",
    female: "女",
    unknown: "未知",
    roster_number: "班内编号",
    join_date: "加入日期",
    withdraw_student: "退出班级",
    transfer_student: "转班",
    batch_operations: "批量操作",
    select_all: "全选",
    deselect_all: "取消全选",
    selected_count: "已选择",
    no_students: "暂无学生",
    no_classrooms: "暂无班级",
    search_placeholder: "搜索...",
    filter_by_classroom: "按班级筛选",
    filter_by_status: "按状态筛选",
  },
  generation: {
    title: "生成任务",
    submit_task: "提交任务",
    task_list: "任务列表",
    task_details: "任务详情",
    provider: "供应商",
    model: "模型",
    prompt: "提示词",
    negative_prompt: "反向提示词",
    parameters: "参数",
    status: "状态",
    pending: "等待中",
    running: "运行中",
    succeeded: "成功",
    failed: "失败",
    cancelled: "已取消",
    timed_out: "已超时",
    progress: "进度",
    started_at: "开始时间",
    completed_at: "完成时间",
    error_message: "错误信息",
    retry: "重试",
    cancel: "取消",
    view_result: "查看结果",
    download_result: "下载结果",
    no_tasks: "暂无任务",
    task_history: "任务历史",
    clear_history: "清除历史",
    export_history: "导出历史",
  },
  assets: {
    title: "资产库",
    import_assets: "导入资产",
    export_assets: "导出资产",
    asset_list: "资产列表",
    asset_details: "资产详情",
    asset_name: "资产名称",
    asset_type: "资产类型",
    asset_size: "资产大小",
    asset_hash: "资产哈希",
    asset_status: "资产状态",
    valid: "有效",
    invalid: "无效",
    missing: "缺失",
    corrupted: "损坏",
    quarantined: "已隔离",
    storage_namespace: "存储空间",
    workspace: "工作空间",
    generation: "生成",
    teaching_resource: "教学资源",
    system: "系统",
    integrity_status: "完整性状态",
    metadata: "元数据",
    created_at: "创建时间",
    updated_at: "更新时间",
    no_assets: "暂无资产",
    search_assets: "搜索资产",
    filter_by_type: "按类型筛选",
    filter_by_status: "按状态筛选",
    reverify_integrity: "重新验证完整性",
    open_containing_folder: "打开所在文件夹",
    delete_asset: "删除资产",
    batch_delete: "批量删除",
    import_progress: "导入进度",
    import_success: "导入成功",
    import_failed: "导入失败",
    skipped_duplicates: "跳过重复",
    skipped_errors: "跳过错误",
  },
  creative: {
    title: "AI 创作",
    creative_workshop: "创意工坊",
    new_project: "新建项目",
    project_name: "项目名称",
    project_description: "项目描述",
    project_status: "项目状态",
    draft: "草稿",
    in_progress: "进行中",
    review: "审核中",
    completed: "已完成",
    archived: "已归档",
    creative_run: "创作运行",
    user_goal: "用户目标",
    workflow_type: "工作流类型",
    current_stage: "当前阶段",
    requirement_analysis: "需求分析",
    visual_spec: "视觉规范",
    story_planning: "故事规划",
    character_planning: "角色规划",
    human_approval: "人工审批",
    image_generation: "图片生成",
    video_generation: "视频生成",
    review_stage: "评价审核",
    optimization: "优化",
    editing: "剪辑",
    final_review: "最终审核",
    export_stage: "导出",
    budget_limit: "预算限制",
    cost_estimate: "成本估算",
    scenes: "场景",
    shots: "镜头",
    characters: "角色",
    scripts: "剧本",
    storyboards: "分镜",
    prompts: "提示词",
    assets_generated: "已生成资产",
    no_projects: "暂无项目",
    no_scenes: "暂无场景",
    no_shots: "暂无镜头",
    no_characters: "暂无角色",
    create_scene: "创建场景",
    create_shot: "创建镜头",
    create_character: "创建角色",
    scene_name: "场景名称",
    shot_description: "镜头描述",
    shot_duration: "镜头时长",
    shot_camera: "镜头类型",
    shot_movement: "运镜方式",
    shot_emotion: "情绪",
    shot_purpose: "镜头目的",
    shot_voiceover: "旁白",
    shot_image_prompt: "图片提示词",
    shot_video_prompt: "视频提示词",
    character_name: "角色名称",
    character_role: "角色角色",
    character_profile: "角色简介",
    character_appearance: "角色外貌",
    character_clothing: "角色服装",
    character_style_lock: "风格锁定",
  },
  critic: {
    title: "AI 评价",
    review_report: "评价报告",
    overall_score: "综合评分",
    decision: "评价决策",
    needs_review: "待审核",
    accept: "可直接采纳",
    accept_with_suggestions: "可采纳（有优化建议）",
    revise: "建议局部修改",
    regenerate: "建议重新生成",
    block: "阻断进入下一阶段",
    requirement_layer: "需求层",
    visual_layer: "视觉层",
    content_layer: "内容层",
    commercial_layer: "商业层",
    technical_layer: "技术层",
    match: "匹配度",
    completeness: "完整度",
    clarity: "清晰度",
    composition: "构图",
    color: "色彩",
    lighting: "光线",
    texture: "质感",
    lens_language: "镜头语言",
    theme_match: "主题匹配",
    emotion_expression: "情绪表达",
    narrative_purpose: "叙事目的",
    platform_fit: "平台适配",
    audience_fit: "受众匹配",
    conversion_potential: "转化潜力",
    distortion: "畸变",
    character_consistency: "角色一致性",
    motion_quality: "运动质量",
    issues_found: "发现的问题",
    no_issues: "暂无问题",
    suggestions: "优化建议",
    confidence: "置信度",
    reviewer_type: "评价类型",
    auto: "自动",
    manual: "手动",
    hybrid: "混合",
    video_generation_gate: "视频生成门槛",
    gate_passed: "已通过",
    gate_failed: "未通过",
    gate_requirements: "门槛要求",
    overall_min: "综合评分最低",
    character_consistency_min: "角色一致性最低",
    style_consistency_min: "风格一致性最低",
    requirement_match_min: "需求匹配度最低",
  },
  edit: {
    title: "修改反馈",
    feedback: "反馈",
    submit_feedback: "提交反馈",
    feedback_placeholder:
      "输入修改反馈，例如：\n• 太假了，增加真实感\n• 不够大气，画面太小了\n• 色调太冷了，暖一点\n• 人物更像创业者，不要像模特",
    analyzing: "分析中...",
    plan_ready: "修改计划就绪",
    apply_plan: "应用修改",
    skip_plan: "跳过",
    plan_summary: "计划摘要",
    operation_type: "操作类型",
    style_adjustment: "风格调整",
    character_adjustment: "人物调整",
    composition_change: "构图变更",
    lighting_adjustment: "光影调整",
    color_grading: "色彩调色",
    camera_change: "镜头变更",
    content_revision: "内容修正",
    quality_improvement: "质量提升",
    regenerate_op: "重新生成",
    prompt_patch: "Prompt 补丁",
    add_keywords: "添加关键词",
    remove_keywords: "移除关键词",
    parameter_patch: "参数补丁",
    model_change: "模型变更",
    steps_change: "步数变更",
    guidance_change: "引导变更",
    requires_regeneration: "需要重新生成",
    requires_critic_rerun: "需要重新评价",
    risk_level: "风险等级",
    low_risk: "低风险",
    medium_risk: "中风险",
    high_risk: "高风险",
    edit_history: "修改历史",
    no_edits: "暂无修改记录",
    edit_applied: "修改已应用",
    edit_skipped: "修改已跳过",
    feedback_received: "反馈已收到",
    intent_parsed: "意图已解析",
    quick_hints: "智能提示",
    smart_suggestions: "智能建议",
  },
  content_guard: {
    title: "内容安全",
    safety_check: "安全检查",
    status: "状态",
    passed: "已通过",
    needs_review: "需人工复核",
    flagged: "已标记风险",
    blocked: "已阻断",
    risk_level: "风险等级",
    low: "低",
    medium: "中",
    high: "高",
    critical: "严重",
    checks_performed: "已执行检查",
    political_sensitivity: "政治敏感",
    nsfw_content: "色情暴力",
    hate_speech: "仇恨言论",
    illegal_content: "违法内容",
    minor_protection: "未成年人保护",
    professional_risk: "专业风险",
    brand_consistency: "品牌一致性",
    platform_compliance: "平台合规",
    educational_safety: "教育安全",
    allowed_actions: "允许的操作",
    blocked_actions: "阻止的操作",
    export_allowed: "允许导出",
    export_blocked: "禁止导出",
    publish_allowed: "允许发布",
    publish_blocked: "禁止发布",
    manual_review_required: "需要人工复核",
    guard_version: "检查版本",
    guard_provider: "检查提供者",
  },
  license: {
    title: "版权管理",
    copyright_status: "版权状态",
    ai_generated: "AI 生成",
    user_uploaded: "用户上传",
    third_party: "第三方素材",
    derived_work: "衍生作品",
    commercial_use: "商用权限",
    clear: "可商用",
    needs_review: "需审核",
    restricted: "受限使用",
    blocked: "禁止商用",
    unknown: "未知",
    export_allowed: "允许导出",
    export_blocked: "禁止导出",
    attribution_required: "需要署名",
    copyright_statement: "版权声明",
    risk_flags: "风险标记",
    third_party_reference: "第三方参考图",
    person_likeness: "人物肖像",
    brand_logo: "品牌标识",
    copyrighted_material: "版权素材",
    music_copyright: "音乐版权",
    font_license: "字体授权",
    provider_info: "供应商信息",
    model_name: "模型名称",
    review_notes: "审核备注",
    no_license: "暂无版权信息",
  },
  memory: {
    title: "创意记忆",
    creative_memory: "创意记忆",
    memory_type: "记忆类型",
    style_preference: "风格偏好",
    negative_preference: "不喜欢的风格",
    brand_rule: "品牌规范",
    workflow_habit: "工作流习惯",
    prompt_pattern: "Prompt 模式",
    review_history: "审核历史",
    scope: "作用域",
    user_scope: "个人",
    project_scope: "项目",
    brand_scope: "品牌",
    add_memory: "添加记忆",
    edit_memory: "编辑记忆",
    delete_memory: "删除记忆",
    pause_memory: "暂停记忆",
    resume_memory: "恢复记忆",
    confirm_memory: "确认记忆",
    memory_summary: "摘要",
    memory_content: "内容",
    confidence: "置信度",
    high_confidence: "高置信",
    medium_confidence: "中置信",
    low_confidence: "低置信",
    active: "生效中",
    paused: "已暂停",
    archived: "已归档",
    no_memories: "暂无创意记忆",
    memory_created: "记忆已创建",
    memory_updated: "记忆已更新",
    memory_deleted: "记忆已删除",
    memory_confirmed: "记忆已确认",
    auto_prefill: "自动预填充",
    style_prefilled: "风格偏好已预填充",
    negative_prefilled: "负面偏好已预填充",
    brand_prefilled: "品牌规范已预填充",
  },
  model_router: {
    title: "模型路由",
    model_selection: "模型选择",
    routing_strategy: "路由策略",
    balanced: "平衡模式",
    cost_optimized: "最低成本",
    quality_first: "最高质量",
    speed_first: "最快速度",
    user_specified: "用户指定",
    available_models: "可用模型",
    selected_model: "已选模型",
    model_provider: "模型供应商",
    model_name: "模型名称",
    capability_score: "能力评分",
    cost_score: "成本评分",
    speed_score: "速度评分",
    quality_score: "质量评分",
    overall_score: "综合评分",
    smart_routing: "智能路由",
    route_decision: "路由决策",
    route_reason: "决策原因",
    requires_confirmation: "需要确认",
    estimated_cost: "预估成本",
    no_models: "暂无可用模型",
    model_unavailable: "模型不可用",
    provider_unavailable: "供应商不可用",
    quota_exceeded: "配额已超",
    rate_limited: "已限流",
  },
  workflow: {
    title: "工作流",
    creative_workflow: "创作工作流",
    start_workflow: "启动工作流",
    advance_workflow: "推进工作流",
    pause_workflow: "暂停工作流",
    resume_workflow: "恢复工作流",
    workflow_status: "工作流状态",
    generating: "生成中",
    evaluating: "自动评价中",
    waiting_feedback: "等待反馈",
    parsing_feedback: "解析反馈中",
    planning_modification: "生成修改计划",
    applying_modification: "执行修改",
    regenerating: "重新生成中",
    completed: "已完成",
    failed: "失败",
    iteration: "迭代次数",
    max_iterations: "最大迭代次数",
    current_stage: "当前阶段",
    workflow_history: "工作流历史",
    no_workflows: "暂无工作流",
    workflow_started: "工作流已启动",
    workflow_advanced: "工作流已推进",
    workflow_paused: "工作流已暂停",
    workflow_resumed: "工作流已恢复",
    workflow_completed: "工作流已完成",
    workflow_failed: "工作流已失败",
    auto_evaluation: "自动评价",
    auto_regenerate: "自动重新生成",
    video_gate_threshold: "视频生成门槛",
    pause_for_feedback: "暂停等待反馈",
  },
  settings: {
    title: "设置",
    general: "通用",
    appearance: "外观",
    theme: "主题",
    light: "浅色",
    dark: "深色",
    system: "跟随系统",
    language: "语言",
    density: "界面密度",
    compact: "紧凑",
    comfortable: "舒适",
    spacious: "宽松",
    navigation: "导航",
    sidebar: "侧边栏",
    topbar: "顶部栏",
    storage: "存储",
    backup: "备份",
    restore: "恢复",
    create_backup: "创建备份",
    restore_backup: "恢复备份",
    backup_history: "备份历史",
    export_data: "导出数据",
    import_data: "导入数据",
    clear_data: "清除数据",
    about: "关于",
    version: "版本",
    build_date: "构建日期",
    license_info: "许可信息",
    check_updates: "检查更新",
    report_issue: "报告问题",
    documentation: "文档",
    support: "支持",
  },
};

// ═══════════════════════════════════════════════════
// 英文翻译
// ═══════════════════════════════════════════════════

export const enUS: TranslationKeys = {
  common: {
    confirm: "Confirm",
    cancel: "Cancel",
    save: "Save",
    delete: "Delete",
    edit: "Edit",
    create: "Create",
    loading: "Loading...",
    error: "Error",
    success: "Success",
    warning: "Warning",
    info: "Info",
    yes: "Yes",
    no: "No",
    ok: "OK",
    back: "Back",
    next: "Next",
    previous: "Previous",
    search: "Search",
    filter: "Filter",
    sort: "Sort",
    refresh: "Refresh",
    close: "Close",
    open: "Open",
    select: "Select",
    clear: "Clear",
    reset: "Reset",
    submit: "Submit",
    retry: "Retry",
    cancel_operation: "Cancel Operation",
    confirm_operation: "Confirm Operation",
    operation_success: "Operation Successful",
    operation_failed: "Operation Failed",
    no_data: "No Data",
    no_results: "No Results",
    loading_data: "Loading data...",
    saving: "Saving...",
    deleting: "Deleting...",
    creating: "Creating...",
    updating: "Updating...",
  },
  auth: {
    login: "Login",
    logout: "Logout",
    register: "Register",
    username: "Username",
    password: "Password",
    email: "Email",
    forgot_password: "Forgot Password",
    reset_password: "Reset Password",
    login_success: "Login Successful",
    login_failed: "Login Failed",
    logout_success: "Logout Successful",
    session_expired: "Session Expired",
    invalid_credentials: "Invalid Username or Password",
    account_locked: "Account Locked",
    account_disabled: "Account Disabled",
    password_changed: "Password Changed",
    password_mismatch: "Password Mismatch",
    username_taken: "Username Already Taken",
    email_taken: "Email Already Taken",
  },
  workspace: {
    title: "Workspace",
    initialize: "Initialize Workspace",
    initialized: "Initialized",
    not_initialized: "Not Initialized",
    name: "Workspace Name",
    teacher_name: "Teacher Name",
    database_status: "Database Status",
    storage_status: "Storage Status",
    backup_status: "Backup Status",
    schema_version: "Schema Version",
    sqlite_version: "SQLite Version",
    journal_mode: "Journal Mode",
    foreign_keys: "Foreign Keys",
    asset_count: "Asset Count",
    total_bytes: "Total Bytes",
    missing_assets: "Missing Assets",
    directories_ready: "Directories Ready",
    writable: "Writable",
    manifest_ready: "Manifest Ready",
  },
  teaching: {
    classrooms: "Classrooms",
    students: "Students",
    teachers: "Teachers",
    classroom: "Classroom",
    student: "Student",
    teacher: "Teacher",
    create_classroom: "Create Classroom",
    edit_classroom: "Edit Classroom",
    delete_classroom: "Delete Classroom",
    classroom_name: "Classroom Name",
    classroom_code: "Classroom Code",
    classroom_description: "Classroom Description",
    teaching_goal: "Teaching Goal",
    start_date: "Start Date",
    end_date: "End Date",
    status: "Status",
    planned: "Planned",
    active: "Active",
    completed: "Completed",
    archived: "Archived",
    student_count: "Student Count",
    add_student: "Add Student",
    import_students: "Import Students",
    export_students: "Export Students",
    student_name: "Student Name",
    student_id: "Student ID",
    student_email: "Email",
    student_phone: "Phone",
    student_gender: "Gender",
    student_notes: "Notes",
    male: "Male",
    female: "Female",
    unknown: "Unknown",
    roster_number: "Roster Number",
    join_date: "Join Date",
    withdraw_student: "Withdraw Student",
    transfer_student: "Transfer Student",
    batch_operations: "Batch Operations",
    select_all: "Select All",
    deselect_all: "Deselect All",
    selected_count: "Selected",
    no_students: "No Students",
    no_classrooms: "No Classrooms",
    search_placeholder: "Search...",
    filter_by_classroom: "Filter by Classroom",
    filter_by_status: "Filter by Status",
  },
  generation: {
    title: "Generation Tasks",
    submit_task: "Submit Task",
    task_list: "Task List",
    task_details: "Task Details",
    provider: "Provider",
    model: "Model",
    prompt: "Prompt",
    negative_prompt: "Negative Prompt",
    parameters: "Parameters",
    status: "Status",
    pending: "Pending",
    running: "Running",
    succeeded: "Succeeded",
    failed: "Failed",
    cancelled: "Cancelled",
    timed_out: "Timed Out",
    progress: "Progress",
    started_at: "Started At",
    completed_at: "Completed At",
    error_message: "Error Message",
    retry: "Retry",
    cancel: "Cancel",
    view_result: "View Result",
    download_result: "Download Result",
    no_tasks: "No Tasks",
    task_history: "Task History",
    clear_history: "Clear History",
    export_history: "Export History",
  },
  assets: {
    title: "Asset Library",
    import_assets: "Import Assets",
    export_assets: "Export Assets",
    asset_list: "Asset List",
    asset_details: "Asset Details",
    asset_name: "Asset Name",
    asset_type: "Asset Type",
    asset_size: "Asset Size",
    asset_hash: "Asset Hash",
    asset_status: "Asset Status",
    valid: "Valid",
    invalid: "Invalid",
    missing: "Missing",
    corrupted: "Corrupted",
    quarantined: "Quarantined",
    storage_namespace: "Storage Namespace",
    workspace: "Workspace",
    generation: "Generation",
    teaching_resource: "Teaching Resource",
    system: "System",
    integrity_status: "Integrity Status",
    metadata: "Metadata",
    created_at: "Created At",
    updated_at: "Updated At",
    no_assets: "No Assets",
    search_assets: "Search Assets",
    filter_by_type: "Filter by Type",
    filter_by_status: "Filter by Status",
    reverify_integrity: "Reverify Integrity",
    open_containing_folder: "Open Containing Folder",
    delete_asset: "Delete Asset",
    batch_delete: "Batch Delete",
    import_progress: "Import Progress",
    import_success: "Import Successful",
    import_failed: "Import Failed",
    skipped_duplicates: "Skipped Duplicates",
    skipped_errors: "Skipped Errors",
  },
  creative: {
    title: "AI Creative",
    creative_workshop: "Creative Workshop",
    new_project: "New Project",
    project_name: "Project Name",
    project_description: "Project Description",
    project_status: "Project Status",
    draft: "Draft",
    in_progress: "In Progress",
    review: "Review",
    completed: "Completed",
    archived: "Archived",
    creative_run: "Creative Run",
    user_goal: "User Goal",
    workflow_type: "Workflow Type",
    current_stage: "Current Stage",
    requirement_analysis: "Requirement Analysis",
    visual_spec: "Visual Spec",
    story_planning: "Story Planning",
    character_planning: "Character Planning",
    human_approval: "Human Approval",
    image_generation: "Image Generation",
    video_generation: "Video Generation",
    review_stage: "Review",
    optimization: "Optimization",
    editing: "Editing",
    final_review: "Final Review",
    export_stage: "Export",
    budget_limit: "Budget Limit",
    cost_estimate: "Cost Estimate",
    scenes: "Scenes",
    shots: "Shots",
    characters: "Characters",
    scripts: "Scripts",
    storyboards: "Storyboards",
    prompts: "Prompts",
    assets_generated: "Assets Generated",
    no_projects: "No Projects",
    no_scenes: "No Scenes",
    no_shots: "No Shots",
    no_characters: "No Characters",
    create_scene: "Create Scene",
    create_shot: "Create Shot",
    create_character: "Create Character",
    scene_name: "Scene Name",
    shot_description: "Shot Description",
    shot_duration: "Shot Duration",
    shot_camera: "Shot Camera",
    shot_movement: "Shot Movement",
    shot_emotion: "Shot Emotion",
    shot_purpose: "Shot Purpose",
    shot_voiceover: "Voiceover",
    shot_image_prompt: "Image Prompt",
    shot_video_prompt: "Video Prompt",
    character_name: "Character Name",
    character_role: "Character Role",
    character_profile: "Character Profile",
    character_appearance: "Character Appearance",
    character_clothing: "Character Clothing",
    character_style_lock: "Style Lock",
  },
  critic: {
    title: "AI Critic",
    review_report: "Review Report",
    overall_score: "Overall Score",
    decision: "Decision",
    needs_review: "Needs Review",
    accept: "Accept",
    accept_with_suggestions: "Accept with Suggestions",
    revise: "Revise",
    regenerate: "Regenerate",
    block: "Block",
    requirement_layer: "Requirement Layer",
    visual_layer: "Visual Layer",
    content_layer: "Content Layer",
    commercial_layer: "Commercial Layer",
    technical_layer: "Technical Layer",
    match: "Match",
    completeness: "Completeness",
    clarity: "Clarity",
    composition: "Composition",
    color: "Color",
    lighting: "Lighting",
    texture: "Texture",
    lens_language: "Lens Language",
    theme_match: "Theme Match",
    emotion_expression: "Emotion Expression",
    narrative_purpose: "Narrative Purpose",
    platform_fit: "Platform Fit",
    audience_fit: "Audience Fit",
    conversion_potential: "Conversion Potential",
    distortion: "Distortion",
    character_consistency: "Character Consistency",
    motion_quality: "Motion Quality",
    issues_found: "Issues Found",
    no_issues: "No Issues",
    suggestions: "Suggestions",
    confidence: "Confidence",
    reviewer_type: "Reviewer Type",
    auto: "Auto",
    manual: "Manual",
    hybrid: "Hybrid",
    video_generation_gate: "Video Generation Gate",
    gate_passed: "Passed",
    gate_failed: "Failed",
    gate_requirements: "Requirements",
    overall_min: "Overall Minimum",
    character_consistency_min: "Character Consistency Minimum",
    style_consistency_min: "Style Consistency Minimum",
    requirement_match_min: "Requirement Match Minimum",
  },
  edit: {
    title: "Edit Feedback",
    feedback: "Feedback",
    submit_feedback: "Submit Feedback",
    feedback_placeholder:
      "Enter edit feedback, e.g.:\n• Too fake, make it more realistic\n• Not grand enough, the image is too small\n• The tone is too cold, make it warmer\n• The character should look more like an entrepreneur, not a model",
    analyzing: "Analyzing...",
    plan_ready: "Plan Ready",
    apply_plan: "Apply Plan",
    skip_plan: "Skip",
    plan_summary: "Plan Summary",
    operation_type: "Operation Type",
    style_adjustment: "Style Adjustment",
    character_adjustment: "Character Adjustment",
    composition_change: "Composition Change",
    lighting_adjustment: "Lighting Adjustment",
    color_grading: "Color Grading",
    camera_change: "Camera Change",
    content_revision: "Content Revision",
    quality_improvement: "Quality Improvement",
    regenerate_op: "Regenerate",
    prompt_patch: "Prompt Patch",
    add_keywords: "Add Keywords",
    remove_keywords: "Remove Keywords",
    parameter_patch: "Parameter Patch",
    model_change: "Model Change",
    steps_change: "Steps Change",
    guidance_change: "Guidance Change",
    requires_regeneration: "Requires Regeneration",
    requires_critic_rerun: "Requires Critic Rerun",
    risk_level: "Risk Level",
    low_risk: "Low Risk",
    medium_risk: "Medium Risk",
    high_risk: "High Risk",
    edit_history: "Edit History",
    no_edits: "No Edit Records",
    edit_applied: "Edit Applied",
    edit_skipped: "Edit Skipped",
    feedback_received: "Feedback Received",
    intent_parsed: "Intent Parsed",
    quick_hints: "Quick Hints",
    smart_suggestions: "Smart Suggestions",
  },
  content_guard: {
    title: "Content Safety",
    safety_check: "Safety Check",
    status: "Status",
    passed: "Passed",
    needs_review: "Needs Review",
    flagged: "Flagged",
    blocked: "Blocked",
    risk_level: "Risk Level",
    low: "Low",
    medium: "Medium",
    high: "High",
    critical: "Critical",
    checks_performed: "Checks Performed",
    political_sensitivity: "Political Sensitivity",
    nsfw_content: "NSFW Content",
    hate_speech: "Hate Speech",
    illegal_content: "Illegal Content",
    minor_protection: "Minor Protection",
    professional_risk: "Professional Risk",
    brand_consistency: "Brand Consistency",
    platform_compliance: "Platform Compliance",
    educational_safety: "Educational Safety",
    allowed_actions: "Allowed Actions",
    blocked_actions: "Blocked Actions",
    export_allowed: "Export Allowed",
    export_blocked: "Export Blocked",
    publish_allowed: "Publish Allowed",
    publish_blocked: "Publish Blocked",
    manual_review_required: "Manual Review Required",
    guard_version: "Guard Version",
    guard_provider: "Guard Provider",
  },
  license: {
    title: "License Management",
    copyright_status: "Copyright Status",
    ai_generated: "AI Generated",
    user_uploaded: "User Uploaded",
    third_party: "Third Party",
    derived_work: "Derived Work",
    commercial_use: "Commercial Use",
    clear: "Clear",
    needs_review: "Needs Review",
    restricted: "Restricted",
    blocked: "Blocked",
    unknown: "Unknown",
    export_allowed: "Export Allowed",
    export_blocked: "Export Blocked",
    attribution_required: "Attribution Required",
    copyright_statement: "Copyright Statement",
    risk_flags: "Risk Flags",
    third_party_reference: "Third Party Reference",
    person_likeness: "Person Likeness",
    brand_logo: "Brand Logo",
    copyrighted_material: "Copyrighted Material",
    music_copyright: "Music Copyright",
    font_license: "Font License",
    provider_info: "Provider Info",
    model_name: "Model Name",
    review_notes: "Review Notes",
    no_license: "No License Info",
  },
  memory: {
    title: "Creative Memory",
    creative_memory: "Creative Memory",
    memory_type: "Memory Type",
    style_preference: "Style Preference",
    negative_preference: "Negative Preference",
    brand_rule: "Brand Rule",
    workflow_habit: "Workflow Habit",
    prompt_pattern: "Prompt Pattern",
    review_history: "Review History",
    scope: "Scope",
    user_scope: "User",
    project_scope: "Project",
    brand_scope: "Brand",
    add_memory: "Add Memory",
    edit_memory: "Edit Memory",
    delete_memory: "Delete Memory",
    pause_memory: "Pause Memory",
    resume_memory: "Resume Memory",
    confirm_memory: "Confirm Memory",
    memory_summary: "Summary",
    memory_content: "Content",
    confidence: "Confidence",
    high_confidence: "High Confidence",
    medium_confidence: "Medium Confidence",
    low_confidence: "Low Confidence",
    active: "Active",
    paused: "Paused",
    archived: "Archived",
    no_memories: "No Memories",
    memory_created: "Memory Created",
    memory_updated: "Memory Updated",
    memory_deleted: "Memory Deleted",
    memory_confirmed: "Memory Confirmed",
    auto_prefill: "Auto Prefill",
    style_prefilled: "Style Prefilled",
    negative_prefilled: "Negative Prefilled",
    brand_prefilled: "Brand Prefilled",
  },
  model_router: {
    title: "Model Router",
    model_selection: "Model Selection",
    routing_strategy: "Routing Strategy",
    balanced: "Balanced",
    cost_optimized: "Cost Optimized",
    quality_first: "Quality First",
    speed_first: "Speed First",
    user_specified: "User Specified",
    available_models: "Available Models",
    selected_model: "Selected Model",
    model_provider: "Model Provider",
    model_name: "Model Name",
    capability_score: "Capability Score",
    cost_score: "Cost Score",
    speed_score: "Speed Score",
    quality_score: "Quality Score",
    overall_score: "Overall Score",
    smart_routing: "Smart Routing",
    route_decision: "Route Decision",
    route_reason: "Route Reason",
    requires_confirmation: "Requires Confirmation",
    estimated_cost: "Estimated Cost",
    no_models: "No Models Available",
    model_unavailable: "Model Unavailable",
    provider_unavailable: "Provider Unavailable",
    quota_exceeded: "Quota Exceeded",
    rate_limited: "Rate Limited",
  },
  workflow: {
    title: "Workflow",
    creative_workflow: "Creative Workflow",
    start_workflow: "Start Workflow",
    advance_workflow: "Advance Workflow",
    pause_workflow: "Pause Workflow",
    resume_workflow: "Resume Workflow",
    workflow_status: "Workflow Status",
    generating: "Generating",
    evaluating: "Evaluating",
    waiting_feedback: "Waiting Feedback",
    parsing_feedback: "Parsing Feedback",
    planning_modification: "Planning Modification",
    applying_modification: "Applying Modification",
    regenerating: "Regenerating",
    completed: "Completed",
    failed: "Failed",
    iteration: "Iteration",
    max_iterations: "Max Iterations",
    current_stage: "Current Stage",
    workflow_history: "Workflow History",
    no_workflows: "No Workflows",
    workflow_started: "Workflow Started",
    workflow_advanced: "Workflow Advanced",
    workflow_paused: "Workflow Paused",
    workflow_resumed: "Workflow Resumed",
    workflow_completed: "Workflow Completed",
    workflow_failed: "Workflow Failed",
    auto_evaluation: "Auto Evaluation",
    auto_regenerate: "Auto Regenerate",
    video_gate_threshold: "Video Gate Threshold",
    pause_for_feedback: "Pause for Feedback",
  },
  settings: {
    title: "Settings",
    general: "General",
    appearance: "Appearance",
    theme: "Theme",
    light: "Light",
    dark: "Dark",
    system: "System",
    language: "Language",
    density: "Density",
    compact: "Compact",
    comfortable: "Comfortable",
    spacious: "Spacious",
    navigation: "Navigation",
    sidebar: "Sidebar",
    topbar: "Topbar",
    storage: "Storage",
    backup: "Backup",
    restore: "Restore",
    create_backup: "Create Backup",
    restore_backup: "Restore Backup",
    backup_history: "Backup History",
    export_data: "Export Data",
    import_data: "Import Data",
    clear_data: "Clear Data",
    about: "About",
    version: "Version",
    build_date: "Build Date",
    license_info: "License Info",
    check_updates: "Check Updates",
    report_issue: "Report Issue",
    documentation: "Documentation",
    support: "Support",
  },
};

// ═══════════════════════════════════════════════════
// 翻译管理器
// ═══════════════════════════════════════════════════

const translations: Record<Locale, TranslationKeys> = {
  "zh-CN": zhCN,
  "en-US": enUS,
};

let currentLocale: Locale = "zh-CN";

export function setLocale(locale: Locale): void {
  currentLocale = locale;
}

export function getLocale(): Locale {
  return currentLocale;
}

export function t(key: string, params?: Record<string, string | number>): string {
  const keys = key.split(".");
  let value: unknown = translations[currentLocale];

  for (const k of keys) {
    if (value && typeof value === "object" && k in value) {
      value = (value as Record<string, unknown>)[k];
    } else {
      // 回退到中文
      value = translations["zh-CN"];
      for (const fallbackKey of keys) {
        if (value && typeof value === "object" && fallbackKey in value) {
          value = (value as Record<string, unknown>)[fallbackKey];
        } else {
          return key; // 键不存在，返回原始键
        }
      }
      break;
    }
  }

  if (typeof value !== "string") {
    return key;
  }

  // 参数替换
  if (params) {
    return value.replace(/\{(\w+)\}/g, (_, paramKey) => {
      return params[paramKey]?.toString() ?? `{${paramKey}}`;
    });
  }

  return value;
}

// ═══════════════════════════════════════════════════
// 便捷访问函数
// ═══════════════════════════════════════════════════

export function common(key: keyof TranslationKeys["common"]): string {
  return t(`common.${key}`);
}

export function auth(key: keyof TranslationKeys["auth"]): string {
  return t(`auth.${key}`);
}

export function workspace(key: keyof TranslationKeys["workspace"]): string {
  return t(`workspace.${key}`);
}

export function teaching(key: keyof TranslationKeys["teaching"]): string {
  return t(`teaching.${key}`);
}

export function generation(key: keyof TranslationKeys["generation"]): string {
  return t(`generation.${key}`);
}

export function assets(key: keyof TranslationKeys["assets"]): string {
  return t(`assets.${key}`);
}

export function creative(key: keyof TranslationKeys["creative"]): string {
  return t(`creative.${key}`);
}

export function critic(key: keyof TranslationKeys["critic"]): string {
  return t(`critic.${key}`);
}

export function edit(key: keyof TranslationKeys["edit"]): string {
  return t(`edit.${key}`);
}

export function contentGuard(key: keyof TranslationKeys["content_guard"]): string {
  return t(`content_guard.${key}`);
}

export function license(key: keyof TranslationKeys["license"]): string {
  return t(`license.${key}`);
}

export function memory(key: keyof TranslationKeys["memory"]): string {
  return t(`memory.${key}`);
}

export function modelRouter(key: keyof TranslationKeys["model_router"]): string {
  return t(`model_router.${key}`);
}

export function workflow(key: keyof TranslationKeys["workflow"]): string {
  return t(`workflow.${key}`);
}

export function settings(key: keyof TranslationKeys["settings"]): string {
  return t(`settings.${key}`);
}
