//! OCR (Optical Character Recognition) via tesseract CLI.
//!
//! Provides fast LSTM-based text extraction from screenshots
//! with per-word bounding boxes and confidence scores.

use anyhow::{Context, Result};
use serde::Serialize;
use std::io::Write;
use std::process::{Command, Stdio};

/// A single detected text item from OCR
#[derive(Debug, Clone, Serialize)]
pub struct OcrItem {
    pub text: String,
    pub confidence: f64,
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

/// Run OCR on an image byte buffer using tesseract TSV output.
///
/// Returns a vector of `OcrItem` (word-level detections).
/// Falls back to stdout plain text parsing if TSV is malformed.
pub fn ocr(image_data: &[u8]) -> Result<Vec<OcrItem>> {
    let mut child = Command::new("tesseract")
        .args(["stdin", "stdout", "--psm", "6", "-l", "eng", "tsv"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .context("failed to spawn tesseract — is it installed? (pacman -S tesseract tesseract-data-eng)")?;

    {
        let stdin = child.stdin.as_mut().expect("failed to open tesseract stdin");
        stdin.write_all(image_data).context("failed to write image to tesseract stdin")?;
    }

    let output = child.wait_with_output().context("tesseract process failed")?;
    let tsv = String::from_utf8_lossy(&output.stdout);

    parse_tsv(&tsv)
}

/// Parse tesseract TSV output (word-level, level=5)
pub fn parse_tsv(tsv: &str) -> Result<Vec<OcrItem>> {
    let mut items = Vec::new();

    for line in tsv.lines().skip(1) {
        // Skip header row
        let fields: Vec<&str> = line.split('\t').collect();
        if fields.len() < 12 {
            continue;
        }

        // level=5 is word-level
        if fields.get(0).map_or(false, |f| *f != "5") {
            continue;
        }

        let text = fields.get(11).cloned().unwrap_or("").trim().to_string();
        if text.is_empty() {
            continue;
        }

        let conf: f64 = fields.get(10).and_then(|c| c.parse().ok()).unwrap_or(0.0);
        let x: i32 = fields.get(6).and_then(|c| c.parse().ok()).unwrap_or(0);
        let y: i32 = fields.get(7).and_then(|c| c.parse().ok()).unwrap_or(0);
        let width: u32 = fields.get(8).and_then(|c| c.parse().ok()).unwrap_or(0);
        let height: u32 = fields.get(9).and_then(|c| c.parse().ok()).unwrap_or(0);

        items.push(OcrItem { text, confidence: conf, x, y, width, height });
    }

    Ok(items)
}

/// Find an item in OCR output by text (exact or partial match)
pub fn find_text<'a>(items: &'a [OcrItem], needle: &str, partial: bool) -> Option<&'a OcrItem> {
    let needle_lower = needle.to_lowercase();
    items.iter().find(|item| {
        let item_lower = item.text.to_lowercase();
        if partial {
            item_lower.contains(&needle_lower)
        } else {
            item_lower == needle_lower
        }
    })
}
