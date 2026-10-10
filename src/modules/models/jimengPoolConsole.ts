/**
 * 即梦账号池「管理控制台」的地址。
 *
 * ⚠️ **是根路径 `/`，不是 `/account-pool/`。** 后者是账号池的 **API 前缀**
 * （`/account-pool/accounts` 等，见代理的 `src/api/routes/account-pool.ts`），
 * 未登录时直接返回 `{"error":"未登录"}`（HTTP 401，`Content-Type: application/json`）。
 * 把它当页面地址开窗口，用户只会看到那句 JSON，**永远到不了控制台的初始化/登录表单**——
 * 这正是 2026-10-10 修掉的「即梦账号登录问题」。
 *
 * 控制台 HTML（标题「即梦 API 管理控制台」，内含 `setup-form` / `login-form`）
 * 由服务端在**根路径**提供；未初始化时显示 setup，已初始化时显示 login。
 */
export const JIMENG_POOL_CONSOLE_URL = "http://127.0.0.1:5100/";

/** 代理存活探测（代理未运行时控制台窗口会白屏，先探再开）。 */
export const JIMENG_POOL_PING_URL = "http://127.0.0.1:5100/ping";

/** 代理端口，与 `src-tauri/src/connectors/resources/jimeng_connector.rs` 保持一致。 */
export const JIMENG_POOL_PORT = 5100;
