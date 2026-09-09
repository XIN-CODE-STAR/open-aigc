#![allow(dead_code)]
//! Composition Facade：合成能力抽象层。
//!
//! 与 GenerationFacade 同构：Skill 只依赖 trait，不知道具体实现（ffmpeg / Remotion / 云服务）。
//!
//! 职责边界：
//! - CompositeSkill 负责：收集 Artifact → 构造 Request → 调用 Facade → 构造 Artifact
//! - CompositionFacade 负责：将多个媒体文件合成为单个视频
//!
//! Future: async worker for long-running composition

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::domain::creative_plan::AssetType;

// ─── Types ───

/// 合成输入项（一个媒体文件）。
#[derive(Debug, Clone)]
pub struct MediaInput {
    /// 本地文件绝对路径。
    pub file_path: String,
    /// 资产类型（Image / Video）。
    pub asset_type: AssetType,
    /// 期望时长（秒），图片默认 3.0。
    pub duration_secs: f64,
}

/// 合成请求。
#[derive(Debug, Clone)]
pub struct CompositionRequest {
    /// 输入媒体列表（已按顺序排列）。
    pub inputs: Vec<MediaInput>,
    /// 输出文件目录。
    pub output_dir: String,
    /// 输出文件名（不含路径）。
    pub output_filename: String,
}

/// 合成输出。
#[derive(Debug, Clone)]
pub struct CompositionOutput {
    /// 合成文件绝对路径。
    pub file_path: String,
    /// 总时长（秒）。
    pub duration_secs: f64,
    /// 文件大小（字节）。
    pub file_size: u64,
    /// 附加元数据。
    pub metadata: HashMap<String, serde_json::Value>,
}

// ─── CompositionFacade Trait ───

/// 合成能力抽象：将多个媒体文件合成为单个视频。
///
/// CompositeSkill 只依赖此 trait，不直接调用 ffmpeg。
pub trait CompositionFacade: Send + Sync {
    fn compose(&self, request: &CompositionRequest) -> Result<CompositionOutput, String>;
}

// ─── FFmpeg Implementation ───

/// 基于 ffmpeg 的合成实现。
///
/// 流程：
/// 1. Normalize：图片 → mp4（-loop 1 -t duration），视频保持原样
/// 2. Concat：所有 mp4 → 最终输出
pub struct FfmpegCompositionFacade {
    ffmpeg_path: String,
}

impl FfmpegCompositionFacade {
    pub fn new(ffmpeg_path: String) -> Self {
        Self { ffmpeg_path }
    }

    /// 将图片转为 mp4 片段（-loop 1 -t duration -vf scale=...）。
    /// 视频文件直接返回原路径（不做 normalize）。
    fn normalize_to_mp4(
        &self,
        input: &MediaInput,
        temp_dir: &Path,
        index: usize,
    ) -> Result<PathBuf, String> {
        match input.asset_type {
            AssetType::Image => {
                let output = temp_dir.join(format!("normalized_{index:03}.mp4"));
                let status = Command::new(&self.ffmpeg_path)
                    .args([
                        "-y",
                        "-loop",
                        "1",
                        "-i",
                        &input.file_path,
                        "-c:v",
                        "libx264",
                        "-t",
                        &format!("{:.1}", input.duration_secs),
                        "-pix_fmt",
                        "yuv420p",
                        "-vf",
                        "scale=1920:1080:force_original_aspect_ratio=decrease,pad=1920:1080:(ow-iw)/2:(oh-ih)/2",
                    ])
                    .arg(output.to_str().unwrap_or("output.mp4"))
                    .stdout(std::process::Stdio::null())
                    .stderr(std::process::Stdio::piped())
                    .status()
                    .map_err(|e| format!("ffmpeg normalize failed: {e}"))?;

                if !status.success() {
                    return Err(format!(
                        "ffmpeg normalize exited with code {:?}",
                        status.code()
                    ));
                }
                Ok(output)
            }
            AssetType::Video => Ok(PathBuf::from(&input.file_path)),
            _ => Err(format!(
                "unsupported asset type for composition: {:?}",
                input.asset_type
            )),
        }
    }

    /// 生成 ffmpeg concat demuxer 所需的 list 文件。
    ///
    /// 格式：每行 `file '/abs/path/to/clip.mp4'`
    fn build_concat_list(normalized_paths: &[PathBuf], list_path: &Path) -> Result<(), String> {
        let mut content = String::new();
        for path in normalized_paths {
            // concat demuxer 要求路径中的单引号转义为 '\''
            let escaped = path.to_str().unwrap_or("").replace('\'', "'\\''");
            content.push_str(&format!("file '{escaped}'\n"));
        }
        std::fs::write(list_path, &content)
            .map_err(|e| format!("failed to write concat list: {e}"))?;
        Ok(())
    }

    /// 执行 ffmpeg concat 拼接。
    fn concat_clips(&self, list_path: &Path, output_path: &Path) -> Result<(), String> {
        let status = Command::new(&self.ffmpeg_path)
            .args([
                "-y",
                "-f",
                "concat",
                "-safe",
                "0",
                "-i",
                list_path.to_str().unwrap_or("list.txt"),
                "-c",
                "copy",
            ])
            .arg(output_path.to_str().unwrap_or("output.mp4"))
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::piped())
            .status()
            .map_err(|e| format!("ffmpeg concat failed: {e}"))?;

        if !status.success() {
            return Err(format!(
                "ffmpeg concat exited with code {:?}",
                status.code()
            ));
        }
        Ok(())
    }
}

impl CompositionFacade for FfmpegCompositionFacade {
    fn compose(&self, request: &CompositionRequest) -> Result<CompositionOutput, String> {
        if request.inputs.is_empty() {
            return Err("no inputs to compose".to_owned());
        }

        // 1. 创建临时目录（normalize 产物存放处）
        let temp_dir = Path::new(&request.output_dir).join("compose_temp");
        std::fs::create_dir_all(&temp_dir)
            .map_err(|e| format!("failed to create temp dir: {e}"))?;

        // 2. Normalize：图片 → mp4，视频保持原样
        let mut normalized_paths: Vec<PathBuf> = Vec::new();
        for (i, input) in request.inputs.iter().enumerate() {
            let path = self.normalize_to_mp4(input, &temp_dir, i)?;
            normalized_paths.push(path);
        }

        // 3. 确保输出目录存在
        std::fs::create_dir_all(&request.output_dir)
            .map_err(|e| format!("failed to create output dir: {e}"))?;

        // 4. 构建输出路径
        let output_path = Path::new(&request.output_dir).join(&request.output_filename);

        // 5. 生成 concat list
        let list_path = temp_dir.join("concat_list.txt");
        Self::build_concat_list(&normalized_paths, &list_path)?;

        // 6. 执行 concat
        self.concat_clips(&list_path, &output_path)?;

        // 7. 读取输出文件信息
        let file_size = std::fs::metadata(&output_path)
            .map_err(|e| format!("output file not found after compose: {e}"))?
            .len();

        let total_duration: f64 = request.inputs.iter().map(|i| i.duration_secs).sum();

        let mut metadata = HashMap::new();
        metadata.insert(
            "engine".to_owned(),
            serde_json::Value::String("ffmpeg".to_owned()),
        );
        metadata.insert(
            "input_count".to_owned(),
            serde_json::Value::Number(request.inputs.len().into()),
        );

        Ok(CompositionOutput {
            file_path: output_path.to_str().unwrap_or("").to_owned(),
            duration_secs: total_duration,
            file_size,
            metadata,
        })
    }
}

// ─── Mock Implementation ───

/// Mock 合成实现（测试用，不执行任何真实操作）。
pub struct MockCompositionFacade;

impl CompositionFacade for MockCompositionFacade {
    fn compose(&self, request: &CompositionRequest) -> Result<CompositionOutput, String> {
        let total_duration: f64 = request.inputs.iter().map(|i| i.duration_secs).sum();
        let output_path = Path::new(&request.output_dir).join(&request.output_filename);

        let mut metadata = HashMap::new();
        metadata.insert(
            "engine".to_owned(),
            serde_json::Value::String("mock".to_owned()),
        );
        metadata.insert(
            "input_count".to_owned(),
            serde_json::Value::Number(request.inputs.len().into()),
        );

        Ok(CompositionOutput {
            file_path: output_path.to_str().unwrap_or("").to_owned(),
            duration_secs: total_duration,
            file_size: 0,
            metadata,
        })
    }
}

// ─── Tests ───

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_concat_list_format() {
        let paths = vec![
            PathBuf::from("/tmp/clip_001.mp4"),
            PathBuf::from("/tmp/clip_002.mp4"),
            PathBuf::from("/tmp/clip_003.mp4"),
        ];

        let temp = std::env::temp_dir().join("test_concat_list.txt");
        FfmpegCompositionFacade::build_concat_list(&paths, &temp).unwrap();

        let content = std::fs::read_to_string(&temp).unwrap();
        assert!(content.contains("file '/tmp/clip_001.mp4'"));
        assert!(content.contains("file '/tmp/clip_002.mp4'"));
        assert!(content.contains("file '/tmp/clip_003.mp4'"));
        assert_eq!(content.lines().count(), 3);

        let _ = std::fs::remove_file(&temp);
    }

    #[test]
    fn test_build_concat_list_escapes_single_quotes() {
        let paths = vec![PathBuf::from("/tmp/it's a clip.mp4")];

        let temp = std::env::temp_dir().join("test_concat_escape.txt");
        FfmpegCompositionFacade::build_concat_list(&paths, &temp).unwrap();

        let content = std::fs::read_to_string(&temp).unwrap();
        // ffmpeg concat demuxer 要求单引号转义为 '\''
        assert!(content.contains("'\\''"));

        let _ = std::fs::remove_file(&temp);
    }

    #[test]
    fn test_mock_compose_success() {
        let facade = MockCompositionFacade;
        let request = CompositionRequest {
            inputs: vec![
                MediaInput {
                    file_path: "/tmp/shot1.mp4".to_owned(),
                    asset_type: AssetType::Video,
                    duration_secs: 5.0,
                },
                MediaInput {
                    file_path: "/tmp/shot2.png".to_owned(),
                    asset_type: AssetType::Image,
                    duration_secs: 3.0,
                },
            ],
            output_dir: "/tmp/output".to_owned(),
            output_filename: "final.mp4".to_owned(),
        };

        let result = facade.compose(&request).unwrap();
        assert_eq!(result.duration_secs, 8.0);
        assert_eq!(result.file_size, 0);
        assert!(result.file_path.contains("final.mp4"));
    }

    #[test]
    fn test_mock_compose_empty_inputs() {
        let facade = MockCompositionFacade;
        let request = CompositionRequest {
            inputs: vec![],
            output_dir: "/tmp/output".to_owned(),
            output_filename: "final.mp4".to_owned(),
        };

        // MockCompositionFacade doesn't check empty (that's CompositeSkill's job)
        let result = facade.compose(&request).unwrap();
        assert_eq!(result.duration_secs, 0.0);
    }
}
