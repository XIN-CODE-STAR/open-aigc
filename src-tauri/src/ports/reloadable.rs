use std::path::Path;

use crate::application::error::AppError;

/// 支持热重载的服务实现此 trait。
/// 批量恢复后，ServiceReloader 会调用 reload 让服务重新打开数据库连接。
pub trait Reloadable: Send + Sync {
    /// 重新打开底层数据库连接。失败时返回 AppError。
    fn reload(&self, database_path: &Path) -> Result<(), AppError>;
}
