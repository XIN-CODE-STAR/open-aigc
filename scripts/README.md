# 辅助脚本

本目录只保存可审阅、可重复的项目检查、数据迁移、GitHub Backlog 和发布辅助脚本。

当前脚本：

- check-prereqs.ps1：只读检查开发工具链，不自动安装或修改系统。
- create-github-issues.ps1：远程仓库建立且 Phase 0 接受后，根据 docs/github-issues.json 创建 Label、Milestone 和 Issue。

规则：

- 脚本默认安全失败，并在执行外部写操作前要求明确参数。
- 不在脚本中硬编码 token、凭据、用户路径或 GitHub owner。
- 破坏性文件操作必须验证绝对路径和工作空间边界。
- 脚本行为变化需要文档、错误处理和可重复验证。
- Phase 0 接受前不得使用脚本生成生产脚手架或远程实施 Issue。
