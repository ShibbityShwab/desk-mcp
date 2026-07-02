//! OCR engine using the `tesseract` CLI binary.
//!
//! Uses the installed `tesseract` command-line tool for recognition.
//! Falls back gracefully when Tesseract is not installed.
//! Much more portable than C library bindings (no ABI compatibility issues).

use anyhow::{Context, Result};
use image::{DynamicImage, ImageFormat};
use serde::{Deserialize, Serialize};
use std::process::Command;

use crate::vision::ClickableRegion;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OcrItem {
    pub text: String,
    pub bounds: Option<OcrBounds>,
    pub confidence: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OcrBounds {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

/// Check if tesseract binary is available.
pub fn is_available() -> bool {
    Command::new("tesseract")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Convenience wrapper: extract text from PNG bytes without region cropping.
pub fn ocr(png_bytes: &[u8]) -> Result<Vec<OcrItem>> {
    extract_text(png_bytes, None)
}

/// Find text in a set of OcrItems, optionally with partial matching.
pub fn find_text<'a>(items: &'a [OcrItem], text: &str, partial: bool) -> Option<&'a OcrItem> {
    let lower = text.to_lowercase();
    items.iter().find(|item| {
        let item_lower = item.text.to_lowercase();
        if partial {
            item_lower.contains(&lower)
        } else {
            item_lower == lower
        }
    })
}

/// Find text at or near specific screen coordinates.
pub fn find_text_at(png_bytes: &[u8], x: i32, y: i32) -> Result<Vec<OcrItem>> {
    use crate::vision::ElementType;
    let region = crate::vision::ClickableRegion {
        x: (x - 50).max(0),
        y: (y - 10).max(0),
        width: 100,
        height: 30,
        element_type: ElementType::Input,
        text: String::new(),
    };
    extract_text(png_bytes, Some(&region))
}

/// Extract text from a PNG screenshot buffer.
/// Returns Vec<OcrItem> with text, bounding boxes, and confidence per word/line.
pub fn extract_text(png_bytes: &[u8], region: Option<&ClickableRegion>) -> Result<Vec<OcrItem>> {
    if !is_available() {
        return Err(anyhow::anyhow!(
            "Tesseract is not installed. Install with: sudo pacman -S tesseract tesseract-data-eng"
        ));
    }

    // Decode PNG to image
    let img = image::load_from_memory(png_bytes)
        .context("OCR: failed to decode PNG image")?;

    let processed = if let Some(r) = region {
        let cropped = img.crop_imm(
            r.x as u32,
            r.y as u32,
            (r.width.max(1)) as u32,
            (r.height.max(1)) as u32,
        );
        preprocess_image(&cropped)
    } else {
        preprocess_image(&img)
    };

    // Write preprocessed image to temp file
    let mut tmp = tempfile::NamedTempFile::new()
        .context("OCR: failed to create temp file")?;
    processed
        .write_to(&mut tmp, ImageFormat::Png)
        .context("OCR: failed to write temp PNG")?;
    let tmp_path = tmp.path().to_string_lossy().to_string();

    // Run tesseract with TSV output for bounding boxes
    let output = Command::new("tesseract")
        .arg(&tmp_path)
        .arg("stdout")
        .arg("-l")
        .arg("eng")
        .arg("--psm")
        .arg("6")
        .arg("tsv")
        .output()
        .context("OCR: failed to run tesseract. Install: sudo pacman -S tesseract tesseract-data-eng")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow::anyhow!("Tesseract failed: {}", stderr.trim()));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let items = parse_tsv(&stdout);

    if items.is_empty() {
        return Err(anyhow::anyhow!("OCR: no text found in image"));
    }

    Ok(items)
}

/// Parse Tesseract TSV output into OcrItem list.
fn parse_tsv(tsv: &str) -> Vec<OcrItem> {
    let mut items = Vec::new();
    for line in tsv.lines().skip(1) {
        let fields: Vec<&str> = line.split('\t').collect();
        if fields.len() < 12 {
            continue;
        }
        let level = fields[0].parse::<u32>().unwrap_or(0);
        if level != 5 {
            continue;
        }
        let text = fields[11].trim().to_string();
        if text.is_empty() {
            continue;
        }
        let left = fields[6].parse::<i32>().unwrap_or(0);
        let top = fields[7].parse::<i32>().unwrap_or(0);
        let width = fields[8].parse::<i32>().unwrap_or(1);
        let height = fields[9].parse::<i32>().unwrap_or(1);
        let conf = fields[10].parse::<f32>().unwrap_or(0.0) / 100.0;

        items.push(OcrItem {
            text,
            bounds: Some(OcrBounds {
                x: left,
                y: top,
                width: width.max(1),
                height: height.max(1),
            }),
            confidence: conf.clamp(0.0, 1.0),
        });
    }
    items
}

/// Preprocess image for better OCR accuracy.
fn preprocess_image(img: &DynamicImage) -> DynamicImage {
    let gray = img.to_luma8();
    let (w, h) = gray.dimensions();
    let scale = if w < 300 || h < 200 { 2.0 } else { 1.0 };
    if scale > 1.0 {
        let resized = image::imageops::resize(
            &gray,
            (w as f64 * scale) as u32,
            (h as f64 * scale) as u32,
            image::imageops::FilterType::Lanczos3,
        );
        DynamicImage::ImageLuma8(resized)
    } else {
        DynamicImage::ImageLuma8(gray)
    }
}
