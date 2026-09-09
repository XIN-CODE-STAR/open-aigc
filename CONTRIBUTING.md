# 贡献指南

## 当前阶段

Phase 0 已于 2026-07-16 接受。当前进入 v0.1 Foundation，所有实现必须遵守已接受架构、Milestone 和分支门槛。

## 分支流程

1. 从 develop 创建分支。
2. 使用 feature/*、fix/*、release/* 或 hotfix/* 命名。
3. 每个分支只处理一个 Issue 或一个明确设计主题。
4. Pull Request 合并到 develop；release/hotfix 按发布流程进入 main。
5. 保持提交小而可评审，不混入无关重构和生成文件。

## Commit 格式

使用 Conventional Commits：

```text
feat: add classroom roster import
fix: handle expired provider account token
docs: document provider adapter contract
refactor: isolate account pool selection
test: cover prompt variable interpolation
build: add tauri release workflow
ci: add lint job
chore: update dependencies
```

文档可以使用更具体 scope：

```text
docs(product): define user journeys
docs(database): design entity relationships
docs(plugin): define permission model
```

## 代码标准

- Provider 特有逻辑只能位于 Adapter 后。
- UI 不直接调用 Provider API、SQLite、文件系统或凭据服务。
- 持久数据访问通过 Application Service 与 Repository。
- Pinia Store 不复制领域数据库，也不保存凭据。
- 长任务必须可观察、可取消并可从重启恢复。
- 重要状态变化发布版本化事件，消费者幂等。
- 优先小模块和显式依赖，禁止重复业务规则。
- 删除 demo、死代码、未使用依赖和无所有权的 shared 工具。
- 重要决策记录在 docs/architecture/，破坏性变化新增 ADR。

## 文档标准

- 中文作为项目主要说明语言，代码标识、仓库名和标准协议保留英文。
- 文档说明 Why、收益、权衡、失败方式和未来扩展。
- 架构文档使用必要图示并保持 Mermaid 可解析。
- SQL、TypeScript 和 JSON 示例标记为设计或生产用途。
- 修改术语、状态或事件时同步检查数据、SDK、UI 和路线文档。
- 每份重要文档使用独立提交。

## Pull Request

每个 PR 必须包含：

- 变更目标与原因。
- 关联 Issue 和 Milestone。
- 实际执行的测试及结果。
- 可见 UI 变化的桌面截图或录屏。
- 数据迁移、文件、Provider、插件、权限、安全和发布影响。
- 明确的非目标与已知限制。

评审清单：

- 变更范围是否可独立评审和回滚。
- 是否遵守领域、Provider、插件和状态边界。
- 是否需要迁移、事件版本或兼容策略。
- 错误、离线、权限、加载和空状态是否完整。
- 是否可能泄露凭据、学生数据、本地路径或工作空间文件。
- 文档、测试和 CHANGELOG 是否需要更新。

## 测试

行为变更按风险增加测试：

- 领域状态机与策略：单元测试。
- Provider/Plugin/IPC：契约测试。
- SQLite、文件、迁移和恢复：集成测试。
- 教师主旅程和桌面布局：Windows E2E 与截图验证。
- Provider Adapter：真实账号的受控成功、故障、限流、取消和恢复测试。

测试数据必须合成或脱敏，不提交真实学生资料、Prompt 私密内容、Provider 凭据或用户工作空间。

## 安全

- 禁止提交 token、Cookie、密钥、证书、真实数据库、备份和下载文件。
- 凭据只能通过安全存储 handle 使用。
- 插件权限新增或扩大必须单独说明并重新审批。
- 发现安全问题时不要公开包含可利用细节的普通 Issue；远程仓库建立后配置私密报告渠道。
