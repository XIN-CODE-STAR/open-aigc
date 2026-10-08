# 即梦非会员账户 · 视频生成能力判断逻辑设计

> 日期：2026-10-08
> 依据：三张即梦 Web UI 截图（两个不同账户的模型下拉）+ 现有 jimeng 连接器代码
> 目的：为非会员账户设计「能否生成视频 / 用哪个模型 / 需要多少积分 / 是否够」的判断逻辑，并落地到 OPEN AIGC 的即梦通道

---

## 一、截图事实梳理

### 账户 A（截图 1，较早/有余额账户）

- 顶部余额：**60 积分**；顶部横幅促销「Seedance 2.5 720p 首月单秒低至 ¥0.27」（说明**按秒计费**）。
- 视频模型清单：`即梦 Seedance 2.0 Fast`（选中）、`即梦 Seedance 2.0`、`即梦 Seedance 1.0`、`即梦 Seedance 1.0 Fast`、`HappyHorse 1.1` —— **无 VIP 锁定项**。
- 参数行：`16:9 · 720P · 15s`，右侧显示 **`+75`** —— 即当前参数下的**积分预估成本**（15s / 720P = 75 积分）。

### 账户 B（截图 2/3，新账户）

- 视频模型清单：`即梦 Seedance 2.0 mini`（选中）、`即梦 Seedance 2.0 Fast VIP`🔒、`即梦 Seedance 2.0 VIP`🔒、`即梦 Seedance 1.0`、`即梦 Seedance 1.0 Fast`、`HappyHorse 1.1`。
- 与 A 的关键差异：
  - **多了 `Seedance 2.0 mini`**（极致性价比档，试用/廉价档）；
  - **少了标准档 `Seedance 2.0 / 2.0 Fast`**，取而代之的是 **VIP 专属的 `2.0 Fast VIP / 2.0 VIP`**（非会员锁定）。

### 关键结论

1. **模型清单是按账户下发的**：同页面同版本，两个账户看到的模型集不同 → 能力判断必须**按账户**，不能全局硬编码。
2. **VIP 模型对非会员锁定**：带 `VIP` 标识 + 蓝色钻石，非会员不可用。
3. **新账户 ≠ 完全不能用视频**：而是**可用模型集不同**（只有 mini）+ **受试用/积分限制**；一旦试用耗尽且无余额，等价于"无法生成"——这正是观察到的现象。
4. **成本 = f(模型, 分辨率, 时长)**：以秒为单位线性叠加，UI 会实时给出预估（`+75`）。
5. **存在"每天 1 次"免费额度**（模型选择器旁标注）。
6. **注册时间只是相关因子**：真正的权威来源应是即梦下发的"账户能力清单"；注册时间用于**解释与兜底**。

---

## 二、账户侧数据模型（需跟踪的字段）

```
AccountVideoProfile {
    membership:      None | Vip | SuperVip          // 会员等级
    registered_at:   Timestamp                      // 注册时间 → account_age_days
    credits: { total, gift, purchase, vip }         // /token/points 已返回 totalCredit
    video_models: [ { id, name, vip_locked } ]      // 账户可用视频模型清单（含锁定标记）
    free_trials: { model: "seedance-2.0-mini", remaining: 0..3 }   // 新账号 3 次试用
    daily_free:  { model, remaining_today, reset_at }             // 每日 1 次
}
```

> 现状对照：`AccountHealth` 已有 `credits_remaining` 与 `membership` 字段，但 jimeng `health_check` **恒填 None**，且 `membership` 被误填成昵称（`membership: Some(nickname)`）——属实现缺陷，需一并修正。

---

## 三、判断逻辑（五步决策）

### Step 0 · 账号可用性前置
```
status != active  →  中断，提示重新登录（need_login / 1015）
```

### Step 1 · 能力闸门：该账户是否支持视频
```
if membership in {Vip, SuperVip}:
    支持（VIP 模型解锁）
else:
    usable = video_models.filter(m => !m.vip_locked)
    if usable 非空:  支持 → 进入 Step 2
    else:            不支持 → 提示「当前账号暂不支持视频生成」
```
- **新账号特例**：`usable` 可能仅含 `seedance-2.0-mini`，且受 `free_trials.remaining` 约束（见 Step 3/4）。

### Step 2 · 模型解析
```
if 请求模型 in usable:            使用该模型
elif 请求模型 被 vip_locked:      提示「会员专属」→ 降级到 usable 最优档
elif 请求模型 不在清单:            降级到 usable 最优档（并回传"实际使用模型"）
```

### Step 3 · 成本计算（按时长）
```
cost = 0
if free_trials.remaining > 0 and model == "seedance-2.0-mini":
    cost = 0                       // 新账号试用，成功提交后 remaining -= 1
elif daily_free.remaining_today > 0 and model == daily_free.model:
    cost = 0                       // 每日免费
else:
    cost = ceil(duration_secs × rate(model, resolution))   // 本地费率表回退
    // 首选：直接向即梦查询预估成本（网页 UI "+75" 的同源接口），最权威
```
- **优先用即梦下发的预估成本**，本地费率表仅作离线回退（避免费率漂移）。

### Step 4 · 余额校验与弹窗
```
if cost == 0:                        放行（标记消耗试用/每日免费）
elif credits.total >= cost:          放行
else:
    弹窗 {
        needed:    cost,
        available: credits.total,
        shortfall: cost - credits.total,
        actions:   [充值, 换更低档模型, 缩短时长, 降分辨率]
    }
```

### Step 5 · 后端错误码兜底（与真实错误对齐）
| 错误 | 含义 | 处理 |
|---|---|---|
| `-2009` / `1006` | 积分不足 | 触发与 Step 4 相同的弹窗 |
| `1310` | 高峰期容量限制 | 提示"稍后重试"（非风控） |
| `2061` | 模型已下线 | 刷新模型清单 |
| `4013` | 风控拦截 | 换通道/模型（2.0 → 2.5） |
| `1015` | session 无效 | 重新登录 |

---

## 四、注册时间假设的处理方式

把"新账号"显式建模为状态，而非散落的 if：
```
NEW_ACCOUNT_DAYS = 7   // 阈值待实测校准
is_new = account_age_days < NEW_ACCOUNT_DAYS
```
- 新账号：`usable = {seedance-2.0-mini}`，`free_trials.remaining = 3`；
- 试用耗尽后：`usable = ∅` → 命中 Step 1 的"不支持视频"分支（与现象一致）；
- 账号"成熟"（注册时间达标或消费达标）后，标准档 `2.0 Fast / 2.0` 解锁。

> **原则**：能力清单是**权威来源**（由即梦下发）；注册时间只用于①清单缺失时的兜底推断，②给用户的解释性文案。不要用注册时间硬判"能不能用"。

---

## 五、集成点（落到 OPEN AIGC）

| 层 | 改动 |
|---|---|
| `AccountHealth` | 扩展 `membership`（真实等级）、`registered_at`、`available_video_models`、`free_trials`；修正 `membership=昵称` 缺陷 |
| `jimeng_connector.health_check` | 改走 `/token/points` 取 `totalCredit`（当前恒 None）；新增模型清单拉取 |
| `account_health_worker` | 已有 `/token/points` 调用 → 扩展解析 totalCredit / membership / 注册时间，回写 resource_accounts |
| `video_generation` 工具 | 提交前插入 Step 1–4 预检，返回结构化结果（`ok` / `insufficient_credits` / `model_locked` / `video_unsupported`）供前端弹窗 |
| 前端 `PromptComposer`（视频模式） | 展示该账户可用模型 + 预估积分 + 余额；不足时弹窗 |
| 模型映射 | `translate_video_model_for_proxy` 补 `seedance-2.0-mini` → 代理键 |

---

## 六、测试矩阵

| 场景 | 期望 |
|---|---|
| 老账户 + 余额充足 | 放行 |
| 老账户 + 余额不足 | 弹窗（含缺口与建议） |
| 新账户 + 试用>0 + mini | 放行，cost=0，试用 −1 |
| 新账户 + 试用=0 | "不支持视频"提示 |
| 非会员选 VIP 模型 | 降级到可用档 / 提示锁定 |
| 每日免费已用尽 | 回落按积分校验 |
| 后端 −2009 | 同一弹窗 |
| session 失效 | 登录提示 |

---

## 七、待确认 / 风险

1. **费率与试用规则需实测**：`rate(model, resolution)` 表、"每天 1 次"的模型归属、3 次试用的确切模型与判定，建议抓取即梦预估成本接口后再定表。当前代码中的费率表为**离线回退占位**（依据截图 15s/720P=75 反推：标准档 5/秒、fast 4/秒、mini 3/秒；1080p ×1.8、480p ×0.5）。
2. **模型清单接口未知**：需确认即梦是否提供"账户可用模型"接口；若无，则退化为按 membership + 注册时间推断。
3. **注册时间字段来源**：`/token/points` 或用户信息接口是否返回注册时间待核实。

## 八、落地实现（2026-10-08）

已实现为纯函数模块 `src-tauri/src/connectors/resources/jimeng_video_capability.rs`：

- 类型：`Membership`、`VideoModel`、`FreeQuota`、`AccountVideoProfile`、`VideoRequest`。
- 决策：`VideoDecision { Proceed{requested_model, used_model, cost, uses_trial, uses_daily_free} | Unsupported{reason} | InsufficientCredits{model, needed, available, shortfall} }`。
- 函数：`usable_models`、`compute_cost`（`ceil(时长 × 费率)`）、`evaluate`（五步）、`map_backend_error`（Step 5 错误码归类）。
- 常量：`NEW_ACCOUNT_DAYS = 7`。
- 模型映射：`translate_video_model_for_proxy` 已补 `seedance-2.0-mini` → `jimeng-video-seedance-2.0-mini`。
- 单测覆盖文档测试矩阵（老账户充足/不足、新账户试用、试用耗尽、VIP 降级/解锁、每日免费、错误码映射、费率基准）。

> 尚未接入：把 `AccountVideoProfile` 的字段真正从即梦接口/`/token/points` 填充，并在 `video_generation` 工具提交前调用 `evaluate` 返回结构化结果供前端弹窗。
