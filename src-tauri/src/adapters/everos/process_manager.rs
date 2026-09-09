//! EverOS 进程管理器。
#![allow(dead_code)]
//!
//! 使用 `std::process::Command` 管理 EverOS 服务子进程。
//! 应用退出时通过 `Drop` 自动终止。

use std::net::TcpStream;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::Mutex;
use std::time::Duration;

use super::EverosError;

/// EverOS 进程管理器。
pub struct EverosProcessManager {
    child: Mutex<Option<Child>>,
    port: u16,
    root_path: PathBuf,
}

impl EverosProcessManager {
    /// 创建新的进程管理器。
    pub fn new(port: u16, root_path: PathBuf) -> Self {
        Self {
            child: Mutex::new(None),
            port,
            root_path,
        }
    }

    /// 启动 EverOS 服务。
    pub fn start(&self) -> Result<(), EverosError> {
        let mut child_guard = self
            .child
            .lock()
            .map_err(|_| EverosError::Process("lock poisoned".to_owned()))?;

        // 已经在运行。
        if child_guard.is_some() {
            return Ok(());
        }

        // 检查根路径。
        if !self.root_path.exists() {
            return Err(EverosError::Process(format!(
                "EverOS 根目录不存在: {}",
                self.root_path.display()
            )));
        }

        // 检查 everos.toml 是否存在。
        let config_path = self.root_path.join("everos.toml");
        if !config_path.exists() {
            return Err(EverosError::Process(format!(
                "EverOS 配置文件不存在: {}，请先运行 setup-everos.ps1",
                config_path.display()
            )));
        }

        // 检查端口是否被占用。
        if self.is_port_in_use() {
            // 端口被占用可能是 EverOS 已经在运行（外部启动的）。
            // 尝试健康检查确认。
            if self.check_health() {
                return Ok(());
            }
            return Err(EverosError::Process(format!(
                "端口 {} 已被其他进程占用",
                self.port
            )));
        }

        // 打开日志文件。
        let log_path = self.root_path.join("everos-server.log");
        let log_file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&log_path)
            .map_err(|e| EverosError::Process(format!("打开日志文件失败: {e}")))?;

        // 启动子进程。
        let stdout_file = log_file
            .try_clone()
            .map_err(|e| EverosError::Process(format!("复制日志文件句柄失败: {e}")))?;

        let child = Command::new("everos")
            .args([
                "server",
                "start",
                "--port",
                &self.port.to_string(),
                "--root",
                self.root_path.to_str().unwrap_or("~/.everos"),
                "--log-level",
                "warning",
            ])
            .stdout(Stdio::from(stdout_file))
            .stderr(Stdio::from(log_file))
            .spawn()
            .map_err(|e| {
                EverosError::Process(format!(
                    "启动 EverOS 失败: {e}。请确认 Python 3.12+ 和 everos 包已安装。"
                ))
            })?;

        *child_guard = Some(child);

        // 等待服务就绪（最多 15 秒，每 500ms 检查一次）。
        drop(child_guard); // 释放锁再等待。
        for _ in 0..30 {
            std::thread::sleep(Duration::from_millis(500));
            if self.check_health() {
                return Ok(());
            }
            // 检查子进程是否已崩溃。
            if !self.is_process_alive() {
                return Err(EverosError::Process(
                    "EverOS 启动后立即退出，请检查日志文件".to_owned(),
                ));
            }
        }

        // 超时但进程还在运行，可能只是启动慢。
        if self.is_process_alive() {
            Ok(())
        } else {
            Err(EverosError::Process(
                "EverOS 启动超时，请检查日志文件".to_owned(),
            ))
        }
    }

    /// 停止 EverOS 服务。
    pub fn stop(&self) -> Result<(), EverosError> {
        let mut child_guard = self
            .child
            .lock()
            .map_err(|_| EverosError::Process("lock poisoned".to_owned()))?;

        if let Some(mut child) = child_guard.take() {
            // 先尝试优雅关闭。
            let _ = child.kill();
            // 等待最多 3 秒。
            let start = std::time::Instant::now();
            while start.elapsed() < Duration::from_secs(3) {
                if child.try_wait().ok().flatten().is_some() {
                    return Ok(());
                }
                std::thread::sleep(Duration::from_millis(100));
            }
            // 强制等待。
            let _ = child.wait();
        }
        Ok(())
    }

    /// 检查子进程是否仍在运行。
    pub fn is_process_alive(&self) -> bool {
        if let Ok(mut guard) = self.child.lock() {
            if let Some(ref mut child) = *guard {
                return child.try_wait().ok().flatten().is_none();
            }
        }
        false
    }

    /// 检查端口是否被占用。
    fn is_port_in_use(&self) -> bool {
        TcpStream::connect_timeout(
            &format!("127.0.0.1:{}", self.port).parse().unwrap(),
            Duration::from_millis(200),
        )
        .is_ok()
    }

    /// 检查 EverOS 健康状态。
    fn check_health(&self) -> bool {
        let url = format!("http://127.0.0.1:{}/health", self.port);
        ureq::get(&url)
            .timeout(Duration::from_secs(2))
            .call()
            .map(|resp| resp.status() == 200)
            .unwrap_or(false)
    }

    /// 获取服务端口。
    pub fn port(&self) -> u16 {
        self.port
    }

    /// 获取根路径。
    pub fn root_path(&self) -> &std::path::Path {
        &self.root_path
    }
}

impl Drop for EverosProcessManager {
    fn drop(&mut self) {
        let _ = self.stop();
    }
}
