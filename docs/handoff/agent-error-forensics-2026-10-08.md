# Agent 失败取证与修复（2026-10-08）

> 范围：一次「执行计划停在 0%、唯一一步红叉」的用户可见故障——从取证到修复的完整记录，
> 以及为此新增的**诊断设施**与**可复用取证流程**。
> 门禁（本提交实测）：Rust **560 passed / 0 failed / 7 ignored**；`cargo clippy --all-targets
> --all-features -- -D warnings` **EXIT=0**；`pnpm check` 通过。
> 提交：`d8cd970`。

---

## 一、故障现象

用户在画布上发一条普通指令：

> 在画布上创建一条便签，记录：项目主线是暗色调科技感

界面表现：**执行计划卡在 0%**，唯一一步带**红色叉**，计划整体 `status=failed`。
**没有任何可读的错误信息**——这正是本次排查最费时的地方。

---

## 二、取证过程（可复用的顺序）

失败在 UI 上不留痕迹，所以必须直接读数据。**顺序很重要**，先读最便宜、区分度最高的信号。

### 第 1 步：锁定失败会话与计划

```python
# workspace = %LOCALAPPDATA%\com.aigcstudio.desktop\workspace\
select id, goal, status, created_at, updated_at, steps_json
from agent_plans order by rowid desc limit 3;
```

得到：`steps_json` 中单步 `status:"failed"`；计划 `created_at` → `updated_at`
**仅 2.88 秒**（`07:01:38.28` → `07:01:41.16`）。**3 秒内死掉的运行，不可能跑过工具。**

### 第 2 步（关键判据）：工具到底有没有被调用

```sql
select count(*) from agent_tool_invocations where conversation_id = '<conv>';
select count(*) from agent_messages         where conversation_id = '<conv>';
```

结果：**工具调用 0 条**，消息**只有那条 user 消息**（assistant 回复、工具结果一条都没落库）。

> **判据：`0` 条 invocation ⇒ 失败发生在「调用工具之前」。**
> 不要去读工具的实现了——它根本没见过这次请求。

这一步把排查范围从「整个工具系统」缩到「LLM 响应 → 消息落库」这一小段。

### 第 3 步：对照组

同一句话在另一个会话（`24b8d87d`）**成功**：完整 4 条消息
（user → assistant+tool_calls → tool 结果 → assistant 收尾），
`canvas_add_note` 调用 `status=succeeded`，`nodeId` 已落库。

**同输入、一成一败** ⇒ 不是输入问题，是**时序/状态**问题。

### 第 4 步：日志给出决定性线索

启动日志尾部：

```
[GrokChat] stream done model=glm-4.5-air finish=tool_calls tool_calls=1 content_len=1
```

`finish=tool_calls` 且 `tool_calls=1` ⇒ **模型确实要求调用工具**；
但 `content_len=1` ⇒ 它同时返回了一个**长度为 1 的 content**。
而数据库里工具从未被调用。

**两个事实拼起来**：LLM 正常返回了工具调用，但**在把响应落库时炸了**，
所以工具永远没被执行。

### 第 5 步：定位到行

`adapters/providers/grok_chat.rs::into_response`：

```rust
// 修复前
let content = if self.content.is_empty() && !tool_calls.is_empty() {
    None
} else {
    Some(self.content)      // ← "\n" 走这里
};
```

`"\n"` **不是** `is_empty()` ⇒ 落到 `Some("\n")` ⇒
`domain::MessageDraft::assistant` 的 `validate_content` 判定
`content.trim().is_empty()` 为真 ⇒ 抛 `Required { field: "content" }`
⇒ `execute_single_step` 的 `Err(..)` 分支把整步标 `Failed` ⇒ 中断。

**工具从未被调用，正是因为异常发生在「构造并持久化 assistant 消息」这一步，
而工具执行排在它后面。**

---

## 三、根因

**一句话：一个用 `is_empty()` 判「空白」的守卫，挡不住 `"\n"`。**

- GLM（`glm-4.5-air`）在返回 `tool_calls` 时，经常同时带一个**纯空白** `content`（`"\n"`）。
- 代码用 `String::is_empty()` 判断「有没有文本」，`"\n"` 长度 1 ⇒ 判定为「有文本」。
- 下游 `validate_content` 用 `trim().is_empty()` 判断 ⇒ 判定为「空白」⇒ 拒绝。
- **两个守卫用了不同的空判据**，于是出现了「上游认为有、下游认为没有」的缝隙，
  异常就从这个缝隙里漏出来，并且**方向完全反了**：
  明明是「工具调用被丢弃」，UI 却显示成「这一步失败了」。

### 为什么它此前完全不可见

失败路径的日志是 `eprintln!("[Agent] plan step {} failed: {e}")`，
但 **Tauri CLI 派生进程的 stderr 不落盘**到 `.workbuddy-ai/dev-launch.log`。
实测确认：日志里**没有**这行 `plan step failed`。
⇒ 在本环境下 `eprintln!` **等于不写**。

**判例：只往 stderr 写错误，等于在这样一个环境里不写错误。**
这是本次故障「发生了但查不出原因」的真正原因，也是必须先补诊断设施的原因。

---

## 四、修复（两层防御）

### 层 1：源头判据改对

`src-tauri/src/adapters/providers/grok_chat.rs`

```rust
let has_text = !self.content.trim().is_empty();   // 原为 self.content.is_empty()
let content = if !has_text { None } else { Some(self.content) };
```

### 层 2：领域层容错（防止别的 provider 再踩）

`src-tauri/src/domain/agent.rs::MessageDraft::assistant`
——带 `tool_calls` 时，纯空白 content **归一化为 `None`**，而不是拒绝：

```rust
let content = match content {
    Some(c) if c.trim().is_empty() && !tool_calls.is_empty() => None,
    other => other,
};
```

即使将来某个 provider 适配器漏改，也不会再因此炸掉整步。

---

## 五、附带修复：同一句话落库 7 次

### 现象

成功会话 `24b8d87d` 里，`便签已创建。` 这一条 assistant 消息**落了 7 条**
（7 个不同 id，时间间隔约 1.7 秒）。全库统计：

```sql
select conversation_id, content, count(*) n from agent_messages
where role='assistant' group by conversation_id, content having n>1 order by n desc;
-- n=7 '\n便签已创建。' / n=6 '\n便签已创建完成。' / n=2 ...
```

### 根因

`execute_single_step`（`execution_engine.rs` 与 `agent_service.rs` 两条执行路径）里，
**每轮迭代都无条件 `append_message` 一条 assistant 消息**。
模型在多轮工具调用之间反复输出**完全相同**的那句话，于是重复落库。

### 修复

内容与「上一条已落库的 assistant 文本」完全一致（trim 后相等且非空）时**跳过落库**，
但**本轮工具调用仍照常执行**；`last_assistant` 保持指向真实存在的记录，
避免 tool 消息挂到不存在的 parent 上。两条执行路径同步修改，行为一致。

跳过时记录一条 `agent.dedup` 诊断（见下），保证「少写了一条」这件事本身可被观察。

---

## 六、新增诊断设施

### 位置与用法

- 模块：`src-tauri/src/application/diagnostics.rs`
- 输出文件：`%LOCALAPPDATA%\com.aigcstudio.desktop\workspace\agent-diagnostics.log`
- 启动时在 `lib.rs` 的 setup 中 `diagnostics::init(&workspace_directory)`

```rust
crate::diag_fail!("agent.step", "plan step {} failed (conv={conversation_id}): {e}", step.index);
```

### 设计要点

| 决定 | 理由 |
|---|---|
| **同步 append**（open/write/close） | 失败路径本就罕见；换来「进程突然中断也不丢最后一行」 |
| **IO 错误一律吞掉** | 诊断绝不能反过来搞崩业务 |
| **未 init 时退化到 stderr** | 不改变原有行为，便于单元测试与早期启动阶段 |
| **不引入 `log`/`tracing` 依赖** | 目标只是「能取证」，朴素实现足够，避免初始化与 filter 配置成本 |

### 已接线的 scope

| scope | 触发点 |
|---|---|
| `agent.step` | 计划步骤失败（`agent_service.rs` 的 `Err(..)` 分支） |
| `agent.tool` | 工具执行失败 |
| `agent.llm` | LLM 远程错误 / 其它 LLM 错误 |
| `agent.dedup` | 跳过重复的 assistant 消息 |

---

## 七、回归测试

新增 3 条，全部锁定本次根因：

| 测试 | 断言 |
|---|---|
| `whitespace_content_with_tool_calls_becomes_none` | 空白 content + 有工具 ⇒ `None`；**并端到端断言归一化后能构造出合法 `MessageDraft`**（正是原故障那一行） |
| `real_content_with_tool_calls_is_preserved` | 反向对照：有真实文本时**必须保留**，不能被误归一化 |
| `diagnostics::tests::log_appends_to_file` | 初始化后确实落盘，且是追加语义 |

> 反向对照是刻意的：只证明「空白被归一化」不足以说明守卫正确——
> 必须同时证明它**没有**把正常内容也吃掉。

---

## 八、判例（可复用）

1. **`is_empty()` 不是空白检查**——`"\n"` 能穿过它。
   守卫必须在**同一个空判据**上达成一致（这里上游 `is_empty()`、下游 `trim().is_empty()`）。
2. **两个守卫用不同空判据 ⇒ 出现缝隙，且失败方向可能完全反了**：
   「工具调用被丢弃」被 UI 呈现成「这一步失败」。
3. **「某步 Failed」≠「工具失败了」**：先查 `agent_tool_invocations` 里**有没有 invocation**。
   `0` 条 ⇒ 失败在调用工具之前，去查响应构造/消息校验，别查工具实现。
4. **只往 stderr 写错误，在派生进程环境里等于不写**：诊断必须落到与宿主重定向无关的位置。
5. **先量化再修**：本次每一步都先取数（2.88 秒 / 0 条 invocation / content_len=1 / n=7），
   才没有在错误的假设上打转。

---

## 九、遗留

- **未在真实 UI 复现验证**：修复经代码路径与回归测试确认，但未在应用内重发该指令做端到端确认。
  下次启动应用后建议实测一次。
- **两套画布仓储并存（架构债）**：
  `memory_nodes` / `memory_canvases`（**UI 实际读**，经 `MemoryCanvasPanel.vue` →
  `bridge/memoryCanvas.ts` → `memory_canvas_v1_*`）与
  `canvas_nodes` / `canvas_canvases`（`bridge/canvas.ts` 的 `canvas_v1_*` 在 `src/` 下**零消费者**）。
  Agent 的 `canvas_add_note` 走 **memory** 那套。
  **查画布数据只看 `memory_*`**；其中一套无人读，属明确的架构债，待收敛。
- **`agent.dedup` 的粒度**：当前按「与上一条完全相同」判重。
  若模型输出**语义相同但字面不同**的句子，仍会各落一条——暂不处理，避免过度设计。
