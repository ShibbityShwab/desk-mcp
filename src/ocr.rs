//! OCR engine using Tesseract 5 via leptess.
//!
//! Tesseract is CPU-only and ~3-5x more accurate on screen text than ocrs.
//! Leptonica handles image preprocessing, Tesseract handles recognition.

use anyhow::{Context, Result};
use image::{DynamicImage, ImageFormat};
use leptess::{LepTess, Variable};
use serde::Serialize;

use crate::vision::ClickableRegion;

#[derive(Debug, Clone, Serialize)]
pub struct OcrItem {
    pub text: String,
    pub bounds: Option<OcrBounds>,
    pub confidence: f32,
}

#[derive(Debug, Clone, Serialize)]
pub struct OcrBounds {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

// Thread-local Tesseract engine for reuse across calls.
std::thread_local! {
    static TESS: std::cell::RefCell<Option<LepTess>> = const { std::cell::RefCell::new(None) };
}

/// Get (or initialize) the thread-local Tesseract engine.
fn with_tess<F, R>(f: F) -> Result<R>
where
    F: FnOnce(&mut LepTess) -> Result<R>,
{
    TESS.with(|cell| {
        let mut opt = cell.borrow_mut();
        if opt.is_none() {
            let mut tess = LepTess::new(Some("eng"), "eng").context(
                "Tesseract: failed to initialize. Is tesseract and eng.traineddata installed?",
            )?;
            // Optimize for screen text — assume uniform block of text
            tess.set_variable(Variable::TesseditPagesegMode, "6")?;
            *opt = Some(tess);
        }
        f(opt.as_mut().unwrap())
    })
}

/// Convenience wrapper: extract text from PNG bytes without region cropping.
/// Used by vision.rs for the main screen_state() flow.
pub fn ocr(png_bytes: &[u8]) -> Result<Vec<OcrItem>> {
    extract_text(png_bytes, None)
}

/// Find text in a set of OcrItems, optionally with partial matching.
/// Returns the first matching item (or None).
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
/// Crops a region around (x, y) and OCRs just that area.
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
    // Decode PNG to image
    let img =
        image::load_from_memory(png_bytes).context("Tesseract: failed to decode PNG image")?;

    let processed = if let Some(r) = region {
        let cropped = img.crop_imm(r.x as u32, r.y as u32, r.width.max(1), r.height.max(1));
        preprocess_image(&cropped)
    } else {
        preprocess_image(&img)
    };

    // Encode to PNG bytes for leptess
    let mut out = Vec::new();
    processed
        .write_to(&mut std::io::Cursor::new(&mut out), ImageFormat::Png)
        .context("Tesseract: failed to re-encode preprocessed image")?;

    with_tess(|tess| {
        tess.set_image_from_mem(&out)
            .context("Tesseract: failed to set image from memory")?;

        // Get word-level bounding boxes
        // RIL_WORD = 3 (page iterator level: word)
        let boxes = tess
            .get_component_boxes(3, true)
            .context("Tesseract: failed to get word bounding boxes")?;

        let n = boxes.get_n();
        if n == 0 {
            // Fallback: get full page text only (no bounding boxes)
            let full_text = tess.get_utf8_text()?;
            let lines: Vec<OcrItem> = full_text
                .lines()
                .filter(|l| !l.trim().is_empty())
                .map(|line| OcrItem {
                    text: line.trim().to_string(),
                    bounds: None,
                    confidence: 0.0,
                })
                .collect();
            return Ok(lines);
        }

        let mut items: Vec<OcrItem> = Vec::new();
        for i in 0..n {
            if let Some(b) = boxes.get_box(i) {
                // Get geometry first (before moving b into leptess Box)
                let mut bx = 0i32;
                let mut by = 0i32;
                let mut bw = 0i32;
                let mut bh = 0i32;
                b.get_geometry(Some(&mut bx), Some(&mut by), Some(&mut bw), Some(&mut bh));

                // Set recognition region to this word box
                let lb = leptess::leptonica::Box { raw: b };
                tess.set_rectangle_from_box(&lb);
                let text = tess.get_utf8_text().unwrap_or_default().trim().to_string();
                if text.is_empty() {
                    continue;
                }
                let conf = (tess.mean_text_conf() as f32 / 100.0).clamp(0.0, 1.0);
                items.push(OcrItem {
                    text,
                    bounds: Some(OcrBounds {
                        x: bx,
                        y: by,
                        width: bw.max(1),
                        height: bh.max(1),
                    }),
                    confidence: conf,
                });
            }
        }

        Ok(items)
    })
}

/// Preprocess image for better OCR accuracy:
/// - Convert to grayscale
/// - Scale up if too small
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
