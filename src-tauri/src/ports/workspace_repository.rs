use crate::domain::workspace::{
    NewWorkspace, UpdateWorkspaceNameResult, WorkspaceProfile, WorkspaceStatus,
};
use crate::ports::persistence::PersistenceError;

pub trait WorkspaceRepository: Send {
    fn get_status(&mut self) -> Result<WorkspaceStatus, PersistenceError>;
    fn initialize(
        &mut self,
        workspace: &NewWorkspace,
    ) -> Result<WorkspaceProfile, PersistenceError>;
    /// 更新工作空间显示名称。未初始化时返回 `UpdateWorkspaceNameResult::NotInitialized`。
    fn update_workspace_name(
        &mut self,
        workspace_name: &str,
    ) -> Result<UpdateWorkspaceNameResult, PersistenceError>;
}
