//! Vision analysis — clickable region detection.
//!
//! Uses `imageproc` edge detection to find rectangular regions in the
//! screenshot that resemble clickable UI elements (buttons, input fields,
//! links, menu items).
//!
//! The `screen_state()` function is used by action handlers to append a
//! structured screen snapshot after every action — giving agents a
//! feedback loop instead of typing blind.

use crate::ocr::{self, OcrItem};
use image::GenericImageView;
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Clickable region
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClickableRegion {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    /// Best-guess element type.
    pub element_type: ElementType,
    /// OCR text inside the region (if any).
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ElementType {
    Button,
    Input,
    Link,
    Checkbox,
    Select,
    Icon,
    MenuItem,
    Unknown,
}

// ---------------------------------------------------------------------------
// Screen state — the core struct returned after every action
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScreenState {
    pub width: u32,
    pub height: u32,
    pub active_window: Option<serde_json::Value>,
    /// All detected text on screen.
    pub text_elements: Vec<OcrItem>,
    /// Detected clickable regions (buttons, inputs, etc.).
    pub clickable_regions: Vec<ClickableRegion>,
    /// Human-readable summary of what's on screen.
    pub description: String,
}

// ---------------------------------------------------------------------------
// Affordances — what the agent can do next
// ---------------------------------------------------------------------------

/// A suggested next action derived from a detected clickable region.
#[derive(Debug, Clone, Serialize)]
pub struct Affordance {
    /// Action name, e.g. "click", "type_into".
    pub action: String,
    /// Human-readable target, e.g. "Save button", "Search input".
    pub target: String,
    /// Suggested parameters for the action (x, y, text, etc.).
    pub params: serde_json::Value,
}

/// Build affordances from detected clickable regions on screen.
///
/// Skips `Unknown` regions and caps output at `max` items.
pub fn build_affordances(regions: &[ClickableRegion], max: usize) -> Vec<Affordance> {
    regions
        .iter()
        .take(max)
        .filter_map(|r| {
            let action = match r.element_type {
                ElementType::Button
                | ElementType::MenuItem
                | ElementType::Icon
                | ElementType::Link => "click",
                ElementType::Input => "type_into",
                ElementType::Checkbox => "click",
                ElementType::Select => "click",
                ElementType::Unknown => return None,
            };
            let target = if r.text.is_empty() {
                format!("{:?} at ({}, {})", r.element_type, r.x, r.y)
            } else {
                r.text.clone()
            };
            Some(Affordance {
                action: action.into(),
                target,
                params: serde_json::json!({
                    "x": r.x + (r.width as i32) / 2,
                    "y": r.y + (r.height as i32) / 2,
                    "text": r.text,
                }),
            })
        })
        .collect()
}

/// Produce a one-sentence human-readable summary of the screen state.
pub fn summarize_screen(state: &ScreenState) -> String {
    let window = state
        .active_window
        .as_ref()
        .and_then(|w| w.get("title").and_then(|t| t.as_str()))
        .unwrap_or("Desktop");
    let text_count = state.text_elements.len();
    let clickable_count = state.clickable_regions.len();

    if clickable_count > 0 {
        let names: Vec<&str> = state
            .clickable_regions
            .iter()
            .filter(|r| !r.text.is_empty())
            .take(3)
            .map(|r| r.text.as_str())
            .collect();
        if names.is_empty() {
            format!(
                "{window}: {text_count} text elements, {clickable_count} clickable regions detected"
            )
        } else {
            format!(
                "{window}: {text_count} text elements, {clickable_count} clickable regions including: {}",
                names.join(", ")
            )
        }
    } else if text_count > 0 {
        format!("{window}: {text_count} text elements detected, no clickable regions")
    } else {
        format!("{window}: no text or clickable elements detected")
    }
}

// ---------------------------------------------------------------------------
// Detection
// ---------------------------------------------------------------------------

/// Analyse a PNG screenshot and return a full `ScreenState`.
pub fn screen_state(
    png: &[u8],
    active_window: Option<serde_json::Value>,
) -> Result<ScreenState, String> {
    let img = image::load_from_memory(png).map_err(|e| format!("decode: {e}"))?;
    let (width, height) = img.dimensions();

    // OCR — already implemented, returns Vec<OcrItem>
    let text_elements = ocr::ocr(png).unwrap_or_default();

    // Detect clickable regions
    let clickable_regions = detect_clickable(&img, &text_elements);

    // Build description
    let description = build_description(&text_elements, &clickable_regions);

    Ok(ScreenState {
        width,
        height,
        active_window,
        text_elements,
        clickable_regions,
        description,
    })
}

// ---------------------------------------------------------------------------
// Clickable region detection via edge density
// ---------------------------------------------------------------------------

fn detect_clickable(img: &image::DynamicImage, texts: &[OcrItem]) -> Vec<ClickableRegion> {
    let gray = img.to_luma8();
    let (w, h) = gray.dimensions();

    // Scan for horizontal edges (top/bottom borders of buttons) and
    // vertical edges (left/right borders).
    let edges = sobel_edges(&gray);

    // Group edge-dense rectangles into candidate regions.
    let candidates = extract_rects(&edges, w, h, 8, 4); // min 8x4 px

    // Classify each candidate by intersecting with OCR text.
    candidates
        .into_iter()
        .map(|(rx, ry, rw, rh)| {
            let text = texts
                .iter()
                .filter(|t| {
                    if let Some(ref b) = t.bounds {
                        rects_overlap((b.x, b.y, b.width, b.height), (rx, ry, rw, rh))
                    } else {
                        false
                    }
                })
                .map(|t| t.text.clone())
                .collect::<Vec<_>>()
                .join(" | ");

            let element_type = classify_region(rx, ry, rw, rh, &text);

            ClickableRegion {
                x: rx,
                y: ry,
                width: rw as u32,
                height: rh as u32,
                element_type,
                text,
            }
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Simple Sobel edge-detection (approximation — fast on CPU)
// ---------------------------------------------------------------------------

fn sobel_edges(gray: &image::GrayImage) -> Vec<bool> {
    let (w, h) = gray.dimensions();
    let wu = w as usize;
    let out_size = (wu * h as usize).max(1);
    let mut edges = vec![false; out_size];

    for y in 1..h.saturating_sub(1) {
        for x in 1..w.saturating_sub(1) {
            let i = (y as usize) * wu + (x as usize);

            let gx = gray.get_pixel(x + 1, y).0[0] as i32 - gray.get_pixel(x - 1, y).0[0] as i32;

            let gy = gray.get_pixel(x, y + 1).0[0] as i32 - gray.get_pixel(x, y - 1).0[0] as i32;

            let mag = (gx.abs() + gy.abs()) as u8;
            edges[i] = mag > 48; // threshold for "edge"
        }
    }

    edges
}

// ---------------------------------------------------------------------------
// Rectangle extraction from edge map
// ---------------------------------------------------------------------------

fn extract_rects(
    edges: &[bool],
    w: u32,
    h: u32,
    min_w: u32,
    min_h: u32,
) -> Vec<(i32, i32, i32, i32)> {
    let wu = w as usize;
    let mut candidates = Vec::new();

    // Scan horizontal edge runs
    for y in 0..h {
        let mut run_start: Option<u32> = None;
        for x in 0..w {
            let i = (y as usize) * wu + (x as usize);
            if edges[i] {
                if run_start.is_none() {
                    run_start = Some(x);
                }
            } else if let Some(sx) = run_start {
                let rw = x - sx;
                if rw >= min_w {
                    // Check for a closing horizontal edge within 30px below
                    for dy in 3..32 {
                        let by = y + dy;
                        if by >= h {
                            break;
                        }
                        let mut has_close = false;
                        for cx in sx..x {
                            let j = (by as usize) * wu + (cx as usize);
                            if cx < w && edges[j] {
                                has_close = true;
                                break;
                            }
                        }
                        if has_close && dy >= min_h {
                            candidates.push((sx as i32, y as i32, rw as i32, dy as i32));
                            break;
                        }
                    }
                }
                run_start = None;
            }
        }
    }

    candidates
}

// ---------------------------------------------------------------------------
// Classification
// ---------------------------------------------------------------------------

fn classify_region(_x: i32, _y: i32, w: i32, h: i32, text: &str) -> ElementType {
    let aspect = w as f32 / (h.max(1) as f32);

    // Heuristic classification
    if aspect > 6.0 && h < 30 {
        ElementType::Input
    } else if aspect > 3.0 && h < 40 {
        ElementType::Button
    } else if h < 20 {
        ElementType::Link
    } else if w < 30 && h < 30 {
        ElementType::Checkbox
    } else if w > 100 && h > 20 && h < 40 {
        if text.contains("⌄") || text.contains("▾") || text.contains("▼") {
            ElementType::Select
        } else {
            ElementType::Button
        }
    } else if w < 50 && h < 50 {
        ElementType::Icon
    } else {
        ElementType::Unknown
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn rects_overlap(a: (i32, i32, i32, i32), b: (i32, i32, i32, i32)) -> bool {
    a.0 < b.0 + b.2 && a.0 + a.2 > b.0 && a.1 < b.1 + b.3 && a.1 + a.3 > b.1
}

fn build_description(texts: &[OcrItem], regions: &[ClickableRegion]) -> String {
    let mut lines = Vec::new();

    if texts.is_empty() && regions.is_empty() {
        return "Empty screen — no text or clickable elements detected.".into();
    }

    let buttons = regions
        .iter()
        .filter(|r| matches!(r.element_type, ElementType::Button))
        .count();
    let inputs = regions
        .iter()
        .filter(|r| matches!(r.element_type, ElementType::Input))
        .count();
    let links = regions
        .iter()
        .filter(|r| matches!(r.element_type, ElementType::Link))
        .count();

    lines.push(format!(
        "Screen: {} text elements, {} buttons, {} inputs, {} links",
        texts.len(),
        buttons,
        inputs,
        links
    ));

    // Top text elements
    for t in texts.iter().take(10) {
        let pos = t
            .bounds
            .as_ref()
            .map(|b| format!("{:.0},{:.0}", b.x, b.y))
            .unwrap_or_else(|| "?".into());
        lines.push(format!(
            "  [{} at ({})] {}",
            match t.confidence {
                c if c > 0.0 => format!("{:.0}%", c * 100.0),
                _ => "?".into(),
            },
            pos,
            t.text
        ));
    }

    if texts.len() > 10 {
        lines.push(format!("  ... +{} more text elements", texts.len() - 10));
    }

    // Clickable regions
    for r in regions.iter().take(5) {
        let label = if r.text.is_empty() {
            "(no text)".to_string()
        } else {
            format!("\"{}\"", r.text.chars().take(30).collect::<String>())
        };
        lines.push(format!(
            "  {:?} at ({},{}) {}×{} {}",
            r.element_type, r.x, r.y, r.width, r.height, label
        ));
    }

    lines.join("\n")
}
