# 辅助脚本

本目录只保存可审阅、可重复的项目检查、数据迁移、GitHub Backlog 和发布辅助脚本。

当前脚本：

- check-prereqs.ps1：只读检查开发工具链，不自动安装或修改系统。
- create-github-issues.ps1：远程仓库建立且 Phase 0 接受后，根据 docs/github-issues.json 创建 Label、Milestone 和 Issue。
- fetch-ffmpeg.mjs：下载并部署 ffmpeg sidecar 到 src-tauri/binaries/（tauri externalBin 分发，视频合成硬依赖）。tauri dev/build 前必须先运行一次；支持 `--zip`（本地包）与 `--url`（换源）。
- channel-health.mjs：三通道（kling / seedance / 即梦代理）健康检查编排。默认只做免费检查；`--submit` 才追加最低档真实生成（计费）。凭据走环境变量（KLING_ACCESS_KEY/KLING_SECRET_KEY/SEEDANCE_API_KEY/JIMENG_SESSION），缺失的通道自动跳过。
- rust-test.mjs：Rust 测试门禁（外部 manifest）。支持透传 cargo test 参数，如 `node scripts/rust-test.mjs -- --ignored channel_health --nocapture` 运行 #[ignore] 的通道冒烟测试。

规则：

- 脚本默认安全失败，并在执行外部写操作前要求明确参数。
- 不在脚本中硬编码 token、凭据、用户路径或 GitHub owner。
- 破坏性文件操作必须验证绝对路径和工作空间边界。
- 脚本行为变化需要文档、错误处理和可重复验证。
- Phase 0 接受前不得使用脚本生成生产脚手架或远程实施 Issue。
