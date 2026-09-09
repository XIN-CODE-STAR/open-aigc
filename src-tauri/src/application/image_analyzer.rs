use std::path::Path;

use crate::application::error::AppError;

/// 图片分析结果：纯本地分析，不消耗 LLM token。
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImageAnalysis {
    pub width: u32,
    pub height: u32,
    pub file_size_bytes: u64,
    pub format: String,
    pub aspect_ratio: String,
    /// 简单的主色调描述（基于像素采样）
    pub dominant_colors: Vec<ColorInfo>,
    /// 自动生成的简短描述
    pub description: String,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ColorInfo {
    pub name: String,
    pub hex: String,
    pub percentage: f32,
}

/// 分析本地图片文件，提取尺寸、格式、主色调等元信息。
/// 不调用任何 LLM，纯本地计算。
pub fn analyze_image(path: &Path) -> Result<ImageAnalysis, AppError> {
    let metadata = std::fs::metadata(path).map_err(|e| AppError::new("图片文件读取失败", e))?;

    let file_size_bytes = metadata.len();

    // 读取图片
    let img = image::open(path).map_err(|e| AppError::new("图片解码失败", e))?;

    let width = img.width();
    let height = img.height();

    // 检测格式
    let format = detect_format(path);

    // 计算宽高比
    let aspect_ratio = simplify_ratio(width, height);

    // 采样主色调（每 10 像素采样一次，降低计算量）
    let dominant_colors = sample_dominant_colors(&img, 8);

    // 生成描述
    let description = format!(
        "{}×{} {} 图片，宽高比 {}，文件大小 {:.1} KB。主色调：{}",
        width,
        height,
        format,
        aspect_ratio,
        file_size_bytes as f64 / 1024.0,
        dominant_colors
            .iter()
            .take(3)
            .map(|c| c.name.as_str())
            .collect::<Vec<_>>()
            .join("、"),
    );

    Ok(ImageAnalysis {
        width,
        height,
        file_size_bytes,
        format,
        aspect_ratio,
        dominant_colors,
        description,
    })
}

fn detect_format(path: &Path) -> String {
    match path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_lowercase())
        .as_deref()
    {
        Some("jpg") | Some("jpeg") => "JPEG".into(),
        Some("png") => "PNG".into(),
        Some("webp") => "WebP".into(),
        Some("gif") => "GIF".into(),
        Some("bmp") => "BMP".into(),
        Some("tiff") | Some("tif") => "TIFF".into(),
        Some(ext) => ext.to_uppercase(),
        None => "未知".into(),
    }
}

fn simplify_ratio(w: u32, h: u32) -> String {
    if w == 0 || h == 0 {
        return "未知".into();
    }
    let g = gcd(w, h);
    format!("{}:{}", w / g, h / g)
}

fn gcd(a: u32, b: u32) -> u32 {
    if b == 0 {
        a
    } else {
        gcd(b, a % b)
    }
}

/// 通过像素采样提取主色调。
/// 将 RGB 空间简化为 8 个色桶，统计出现频率。
fn sample_dominant_colors(img: &image::DynamicImage, max_colors: usize) -> Vec<ColorInfo> {
    let rgb = img.to_rgb8();
    let (w, h) = rgb.dimensions();
    let step = 10u32.max(w.max(h) / 100); // 采样间隔

    let mut buckets = [0u32; 8]; // 0=红,1=橙,2=黄,3=绿,4=青,5=蓝,6=紫,7=中性
    let mut total = 0u32;

    for y in (0..h).step_by(step as usize) {
        for x in (0..w).step_by(step as usize) {
            let pixel = rgb.get_pixel(x, y);
            let bucket = classify_color(pixel[0], pixel[1], pixel[2]);
            buckets[bucket] += 1;
            total += 1;
        }
    }

    if total == 0 {
        return vec![];
    }

    let color_defs: Vec<(&str, &str)> = vec![
        ("红色", "#e74c3c"),
        ("橙色", "#e67e22"),
        ("黄色", "#f1c40f"),
        ("绿色", "#2ecc71"),
        ("青色", "#1abc9c"),
        ("蓝色", "#3498db"),
        ("紫色", "#9b59b6"),
        ("灰色", "#95a5a6"),
    ];

    let mut result: Vec<ColorInfo> = buckets
        .iter()
        .enumerate()
        .filter(|(_, &count)| count > 0)
        .map(|(i, &count)| ColorInfo {
            name: color_defs[i].0.to_string(),
            hex: color_defs[i].1.to_string(),
            percentage: (count as f32 / total as f32) * 100.0,
        })
        .collect();

    result.sort_by(|a, b| b.percentage.partial_cmp(&a.percentage).unwrap());
    result.truncate(max_colors);
    result
}

fn classify_color(r: u8, g: u8, b: u8) -> usize {
    let (r, g, b) = (r as i32, g as i32, b as i32);
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let saturation = if max == 0 { 0 } else { (max - min) * 255 / max };

    // 低饱和度 = 中性色
    if saturation < 30 {
        return 7; // 灰色
    }

    // 按色相判断
    if r > g && r > b {
        if g > b + 40 {
            1 // 橙色
        } else {
            0 // 红色
        }
    } else if g > r && g > b {
        3 // 绿色
    } else if b > r && b > g {
        if r > g {
            6 // 紫色
        } else {
            5 // 蓝色
        }
    } else if r > b && g > b {
        2 // 黄色
    } else {
        4 // 青色
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simplify_ratio() {
        assert_eq!(simplify_ratio(1920, 1080), "16:9");
        assert_eq!(simplify_ratio(1024, 1024), "1:1");
        assert_eq!(simplify_ratio(0, 100), "未知");
    }

    #[test]
    fn test_classify_color() {
        assert_eq!(classify_color(255, 0, 0), 0); // 红色
        assert_eq!(classify_color(0, 255, 0), 3); // 绿色
        assert_eq!(classify_color(0, 0, 255), 5); // 蓝色
        assert_eq!(classify_color(128, 128, 128), 7); // 灰色
    }
}
