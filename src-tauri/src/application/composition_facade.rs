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

    // ─── 真机合成测试（P0.2）───
    //
    // 依赖 ffmpeg（sidecar 或 PATH），缺失时跳过。部署命令：
    // node scripts/fetch-ffmpeg.mjs
    //
    // 夹具与校验刻意全部经由 FfmpegCompositionFacade 自身完成：
    // 图片夹具用 image crate 落 PNG，片段通过 normalize 分支生成，
    // 时长断言直接解析 mp4 容器（moov/mvhd），测试代码不新增进程调用。

    /// 集成测试用 ffmpeg 定位：运行时探测（sidecar/PATH）→ 仓库 binaries/ 目录。
    /// cargo test 的可执行文件在 target/debug/deps，探测不到尚未被
    /// tauri dev/build 复制的 sidecar，因此补 CARGO_MANIFEST_DIR/binaries 回退
    /// （fetch-ffmpeg.mjs 的部署位置）。
    fn test_ffmpeg_path() -> Option<String> {
        if let Some(path) = crate::application::composite_skill::detect_ffmpeg() {
            return Some(path);
        }
        let candidate = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("binaries")
            .join(format!(
                "ffmpeg-x86_64-pc-windows-msvc{}",
                std::env::consts::EXE_SUFFIX
            ));
        if crate::application::composite_skill::looks_like_ffmpeg_binary(&candidate) {
            return Some(candidate.to_string_lossy().to_string());
        }
        None
    }

    /// 生成纯色 PNG 夹具。
    fn write_solid_png(path: &Path, rgb: [u8; 3]) {
        image::RgbImage::from_pixel(320, 240, image::Rgb(rgb))
            .save(path)
            .expect("PNG 夹具写入失败");
    }

    /// 从 mp4 的 moov/mvhd box 读取实际时长（秒）。
    /// concat -c copy 的产物时长应等于各输入容器时长之和；
    /// 直接读容器元数据而不是信任 facade 的估算值。
    fn mp4_duration_secs(path: &Path) -> Option<f64> {
        let data = std::fs::read(path).ok()?;
        let mut i = 0usize;
        while i + 8 <= data.len() {
            let size = u32::from_be_bytes(data[i..i + 4].try_into().ok()?) as usize;
            let box_type = &data[i + 4..i + 8];
            let (body_start, total) = if size == 0 {
                (i + 8, data.len() - i)
            } else {
                (i + 8, size)
            };
            if total < 8 || i + total > data.len() {
                return None;
            }
            if box_type == b"moov" {
                let moov_end = i + total;
                let mut j = body_start;
                while j + 8 <= moov_end {
                    let s2 = u32::from_be_bytes(data[j..j + 4].try_into().ok()?) as usize;
                    let t2 = &data[j + 4..j + 8];
                    let (b2, tot2) = if s2 == 0 {
                        (j + 8, moov_end - j)
                    } else {
                        (j + 8, s2)
                    };
                    if tot2 < 32 || j + tot2 > moov_end {
                        return None;
                    }
                    if t2 == b"mvhd" {
                        let version = data[b2];
                        if version == 1 {
                            // v1: version/flags(4) ctime(8) mtime(8) timescale(4) duration(8)
                            let ts =
                                u32::from_be_bytes(data[b2 + 20..b2 + 24].try_into().ok()?) as f64;
                            let dur =
                                u64::from_be_bytes(data[b2 + 24..b2 + 32].try_into().ok()?) as f64;
                            return if ts > 0.0 { Some(dur / ts) } else { None };
                        }
                        // v0: version/flags(4) ctime(4) mtime(4) timescale(4) duration(4)
                        let ts = u32::from_be_bytes(data[b2 + 12..b2 + 16].try_into().ok()?) as f64;
                        let dur =
                            u32::from_be_bytes(data[b2 + 16..b2 + 20].try_into().ok()?) as f64;
                        return if ts > 0.0 { Some(dur / ts) } else { None };
                    }
                    j += tot2;
                }
                return None;
            }
            i += total;
        }
        None
    }

    #[test]
    fn test_ffmpeg_compose_two_video_clips_real() {
        let ffmpeg = match test_ffmpeg_path() {
            Some(p) => p,
            None => {
                eprintln!("skip: 未检测到 ffmpeg，先运行 scripts/fetch-ffmpeg.mjs 部署 sidecar");
                return;
            }
        };
        let dir = std::env::temp_dir().join(format!("aigc_compose_test_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let dir_str = dir.to_string_lossy().to_string();
        let facade = FfmpegCompositionFacade::new(ffmpeg);

        // 夹具：两张纯色图经 normalize 分支生成同规格 mp4 片段（2s / 3s）。
        // 同一 facade、同一转码参数 → concat -c copy 可行。
        let make_clip = |name: &str, rgb: [u8; 3], secs: f64| {
            let png = dir.join(format!("{name}.png"));
            write_solid_png(&png, rgb);
            facade
                .compose(&CompositionRequest {
                    inputs: vec![MediaInput {
                        file_path: png.to_string_lossy().to_string(),
                        asset_type: AssetType::Image,
                        duration_secs: secs,
                    }],
                    output_dir: dir_str.clone(),
                    output_filename: format!("{name}.mp4"),
                })
                .expect("夹具片段生成应成功")
        };
        make_clip("clip1", [180, 40, 40], 2.0);
        make_clip("clip2", [40, 40, 180], 3.0);
        let clip1 = dir.join("clip1.mp4");
        let clip2 = dir.join("clip2.mp4");
        assert!(clip1.exists() && clip2.exists());

        // 被测路径：两个视频输入的 concat 拼接
        let output = facade
            .compose(&CompositionRequest {
                inputs: vec![
                    MediaInput {
                        file_path: clip1.to_string_lossy().to_string(),
                        asset_type: AssetType::Video,
                        duration_secs: 2.0,
                    },
                    MediaInput {
                        file_path: clip2.to_string_lossy().to_string(),
                        asset_type: AssetType::Video,
                        duration_secs: 3.0,
                    },
                ],
                output_dir: dir_str.clone(),
                output_filename: "final.mp4".to_owned(),
            })
            .expect("真实 ffmpeg 合成应成功");

        assert!(output.file_size > 0, "合成产物不应为空文件");
        let final_path = Path::new(&output.file_path);
        assert!(final_path.exists(), "合成产物应落盘: {}", output.file_path);
        let real = mp4_duration_secs(final_path).expect("应能从 mp4 容器解析出实际时长");
        assert!(
            (real - 5.0).abs() < 0.5,
            "拼接产物实际时长 {real}s 应≈两段之和 5s"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_ffmpeg_compose_image_input_real() {
        let ffmpeg = match test_ffmpeg_path() {
            Some(p) => p,
            None => {
                eprintln!("skip: 未检测到 ffmpeg，先运行 scripts/fetch-ffmpeg.mjs 部署 sidecar");
                return;
            }
        };
        let dir = std::env::temp_dir().join(format!("aigc_compose_img_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();

        let image = dir.join("still.png");
        write_solid_png(&image, [40, 160, 90]);

        // 被测路径：图片输入走 normalize_to_mp4（-loop 1 -t duration）
        let facade = FfmpegCompositionFacade::new(ffmpeg);
        let output = facade
            .compose(&CompositionRequest {
                inputs: vec![MediaInput {
                    file_path: image.to_string_lossy().to_string(),
                    asset_type: AssetType::Image,
                    duration_secs: 2.0,
                }],
                output_dir: dir.to_string_lossy().to_string(),
                output_filename: "final.mp4".to_owned(),
            })
            .expect("图片归一化合成应成功");

        assert!(output.file_size > 0, "合成产物不应为空文件");
        let real =
            mp4_duration_secs(Path::new(&output.file_path)).expect("应能从 mp4 容器解析出实际时长");
        assert!(
            (real - 2.0).abs() < 0.5,
            "图片归一化产物实际时长 {real}s 应≈声明的 2s"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }
}
