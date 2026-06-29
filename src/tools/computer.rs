//! Computer use tool handlers — 24 tools for desktop control.

use crate::response::{self, ToolResponse};
use crate::PROVIDER;
use base64::Engine;
use serde_json::Value;
use std::time::Duration;

/// Capture a post-action screenshot + OCR + clickable detection.
async fn post_action_screen() -> Result<crate::vision::ScreenState, String> {
    let png = PROVIDER
        .screenshot(None)
        .map_err(|e| format!("screenshot failed: {e}"))?;
    let active_window = PROVIDER.get_active_window().ok().flatten().map(|w| {
        serde_json::json!({
            "title": w.title,
            "app": w.app,
            "geometry": {"x": w.geometry.x, "y": w.geometry.y, "width": w.geometry.width, "height": w.geometry.height}
        })
    });
    crate::vision::screen_state(&png, active_window)
}

/// Merge post-action screen state into a tool result (silently on error).
async fn with_screen(v: serde_json::Value) -> Result<serde_json::Value, (String, String)> {
    let mut obj = v.as_object().cloned().unwrap_or_default();
    match post_action_screen().await {
        Ok(state) => {
            let affordances = crate::vision::build_affordances(&state.clickable_regions, 10);
            let summary = crate::vision::summarize_screen(&state);
            obj.insert("affordances".into(), serde_json::to_value(&affordances).unwrap_or_default());
            obj.insert("screen_summary".into(), serde_json::Value::String(summary));
            obj.insert("screen".into(), serde_json::to_value(&state).unwrap_or_default());
        }
        Err(e) => {
            obj.insert("screen_error".into(), serde_json::Value::String(e));
        }
    }
    Ok(serde_json::Value::Object(obj))
}

/// Handle all computer use tool calls
pub async fn handle(name: &str, args: Value) -> ToolResponse {
    let result = handle_inner(name, args).await;
    match result {
        Ok(value) => response::ok(value),
        Err((code, message)) => response::err(&code, &message),
    }
}

/// Inner handler. Returns `Err((code, message))` so individual tools can
/// surface a distinct error code (e.g. `SHELL_DISABLED`) when needed.
async fn handle_inner(name: &str, args: Value) -> Result<Value, (String, String)> {
    let p = &PROVIDER;

    macro_rules! map_err {
        ($expr:expr, $msg:literal) => {
            $expr.map_err(|e| ("COMPUTER_ERROR".into(), format!($msg, e)))
        };
    }

    match name {
        "screenshot" => {
            let region = args.get("region").and_then(|r| {
                let arr = r.as_array()?;
                Some((
                    arr.first()?.as_i64()? as i32,
                    arr.get(1)?.as_i64()? as i32,
                    arr.get(2)?.as_u64()? as u32,
                    arr.get(3)?.as_u64()? as u32,
                ))
            });

            let img_bytes = map_err!(p.screenshot(region), "screenshot failed: {0}")?;

            let decoded = image::load_from_memory(&img_bytes)
                .map_err(|e| ("COMPUTER_ERROR".into(), format!("image decode failed: {e}")))?;

            let b64 = base64::engine::general_purpose::STANDARD.encode(&img_bytes);

            Ok(serde_json::json!({
                "image_base64": b64,
                "width": decoded.width(),
                "height": decoded.height(),
                "format": "png",
                "size_bytes": img_bytes.len(),
            }))
        }

        "get_screen_size" => {
            let size = map_err!(p.get_screen_size(), "get_screen_size failed: {0}")?;
            Ok(serde_json::json!({ "width": size.width, "height": size.height }))
        }

        "mouse_move" => {
            let x = args["x"].as_i64().unwrap_or(0) as i32;
            let y = args["y"].as_i64().unwrap_or(0) as i32;
            let smooth = args
                .get("smooth")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            let dur = args
                .get("duration_ms")
                .and_then(|v| v.as_u64())
                .unwrap_or(200);

            map_err!(p.mouse_move(x, y, smooth, dur), "mouse_move failed: {0}")?;
            let result = serde_json::json!({"moved_to": {"x": x, "y": y}});
            with_screen(result).await
        }

        "mouse_click" => {
            let button = args
                .get("button")
                .and_then(|v| v.as_str())
                .unwrap_or("left");
            let x = args.get("x").and_then(|v| v.as_i64()).map(|v| v as i32);
            let y = args.get("y").and_then(|v| v.as_i64()).map(|v| v as i32);
            let clicks = args.get("clicks").and_then(|v| v.as_u64()).unwrap_or(1) as u32;

            map_err!(
                p.mouse_click(button, x, y, clicks),
                "mouse_click failed: {0}"
            )?;
            with_screen(serde_json::json!({})).await
        }

        "mouse_double_click" => {
            let button = args
                .get("button")
                .and_then(|v| v.as_str())
                .unwrap_or("left");
            let x = args.get("x").and_then(|v| v.as_i64()).map(|v| v as i32);
            let y = args.get("y").and_then(|v| v.as_i64()).map(|v| v as i32);

            map_err!(
                p.mouse_click(button, x, y, 2),
                "mouse_double_click failed: {0}"
            )?;
            with_screen(serde_json::json!({})).await
        }

        "mouse_scroll" => {
            let dx = args.get("dx").and_then(|v| v.as_i64()).unwrap_or(0) as i32;
            let dy = args.get("dy").and_then(|v| v.as_i64()).unwrap_or(0) as i32;
            let x = args.get("x").and_then(|v| v.as_i64()).map(|v| v as i32);
            let y = args.get("y").and_then(|v| v.as_i64()).map(|v| v as i32);

            map_err!(p.mouse_scroll(dx, dy, x, y), "mouse_scroll failed: {0}")?;
            with_screen(serde_json::json!({})).await
        }

        "mouse_drag" => {
            let x1 = args["x1"].as_i64().unwrap_or(0) as i32;
            let y1 = args["y1"].as_i64().unwrap_or(0) as i32;
            let x2 = args["x2"].as_i64().unwrap_or(0) as i32;
            let y2 = args["y2"].as_i64().unwrap_or(0) as i32;
            let button = args
                .get("button")
                .and_then(|v| v.as_str())
                .unwrap_or("left");
            let dur = args
                .get("duration_ms")
                .and_then(|v| v.as_u64())
                .unwrap_or(500);

            map_err!(
                p.mouse_drag(x1, y1, x2, y2, button, dur),
                "mouse_drag failed: {0}"
            )?;
            with_screen(serde_json::json!({})).await
        }

        "keyboard_type" => {
            let text = args["text"].as_str().unwrap_or("");
            let delay = args.get("delay_ms").and_then(|v| v.as_u64()).unwrap_or(10);

            map_err!(p.keyboard_type(text, delay), "keyboard_type failed: {0}")?;
            let result = serde_json::json!({"chars_typed": text.len()});
            with_screen(result).await
        }

        "key_press" => {
            let key = args["key"].as_str().unwrap_or("");
            map_err!(p.key_press(key), "key_press failed: {0}")?;
            let result = serde_json::json!({"key": key});
            with_screen(result).await
        }

        "press_hotkey" => {
            let keys: Vec<String> = args["keys"]
                .as_array()
                .map(|a| {
                    a.iter()
                        .filter_map(|v| v.as_str().map(String::from))
                        .collect()
                })
                .unwrap_or_default();

            let combo = keys.join("+");
            map_err!(p.key_press(&combo), "press_hotkey failed: {0}")?;
            let result = serde_json::json!({"combo": combo});
            with_screen(result).await
        }

        "click_on_text" => {
            let text = args["text"].as_str().unwrap_or("").to_string();
            let button = args
                .get("button")
                .and_then(|v| v.as_str())
                .unwrap_or("left");
            let partial = args
                .get("partial")
                .and_then(|v| v.as_bool())
                .unwrap_or(true);

            let region = args.get("region").and_then(|r| {
                let arr = r.as_array()?;
                Some((
                    arr.first()?.as_i64()? as i32,
                    arr.get(1)?.as_i64()? as i32,
                    arr.get(2)?.as_u64()? as u32,
                    arr.get(3)?.as_u64()? as u32,
                ))
            });

            let img_bytes = map_err!(p.screenshot(region), "screenshot failed: {0}")?;
            let items = map_err!(crate::ocr::ocr(&img_bytes), "ocr failed: {0}")?;

            let found = crate::ocr::find_text(&items, &text, partial).ok_or_else(|| {
                (
                    "TEXT_NOT_FOUND".into(),
                    format!("Text '{text}' not found on screen"),
                )
            })?;

            let fb = found
                .bounds
                .as_ref()
                .map(|b| (b.x + b.width / 2, b.y + b.height / 2))
                .unwrap_or((0, 0));
            let (px, py) = fb;

            map_err!(
                p.mouse_click(button, Some(px), Some(py), 1),
                "click failed: {0}"
            )?;

            let result = serde_json::json!({
                "text": found.text,
                "confidence": found.confidence,
                "clicked_at": [px, py],
            });
            with_screen(result).await
        }

        "wait_for_text" => {
            let text = args["text"].as_str().unwrap_or("");
            let timeout = args.get("timeout").and_then(|v| v.as_f64()).unwrap_or(10.0);
            let partial = args
                .get("partial")
                .and_then(|v| v.as_bool())
                .unwrap_or(true);

            let region = args.get("region").and_then(|r| {
                let arr = r.as_array()?;
                Some((
                    arr.first()?.as_i64()? as i32,
                    arr.get(1)?.as_i64()? as i32,
                    arr.get(2)?.as_u64()? as u32,
                    arr.get(3)?.as_u64()? as u32,
                ))
            });

            let start = std::time::Instant::now();
            while start.elapsed().as_secs_f64() < timeout {
                let img_bytes = map_err!(p.screenshot(region), "screenshot failed: {0}")?;

                if let Ok(items) = crate::ocr::ocr(&img_bytes) {
                    if let Some(found) = crate::ocr::find_text(&items, text, partial) {
                        let fb2 = found
                            .bounds
                            .as_ref()
                            .map(|b| (b.x + b.width / 2, b.y + b.height / 2))
                            .unwrap_or((0, 0));
                        return Ok(serde_json::json!({
                            "text": found.text,
                            "confidence": found.confidence,
                            "position": [fb2.0, fb2.1],
                            "waited_ms": start.elapsed().as_millis(),
                        }));
                    }
                }
                tokio::time::sleep(Duration::from_millis(500)).await;
            }

            Err((
                "TEXT_NOT_FOUND".into(),
                format!("'{text}' not found within {timeout}s"),
            ))
        }

        "extract_text" => {
            let region = args.get("region").and_then(|r| {
                let arr = r.as_array()?;
                Some((
                    arr.first()?.as_i64()? as i32,
                    arr.get(1)?.as_i64()? as i32,
                    arr.get(2)?.as_u64()? as u32,
                    arr.get(3)?.as_u64()? as u32,
                ))
            });

            let img_bytes = map_err!(p.screenshot(region), "screenshot failed: {0}")?;
            let items = map_err!(crate::ocr::ocr(&img_bytes), "ocr failed: {0}")?;

            Ok(serde_json::json!({
                "items": items.iter().map(|i| serde_json::json!({
                    "text": i.text,
                    "confidence": i.confidence,
                    "bbox": i.bounds.as_ref().map(|b| [b.x, b.y, b.width, b.height]),
                })).collect::<Vec<_>>(),
                "count": items.len(),
            }))
        }

        "describe_screen" => {
            let region = args.get("region").and_then(|r| {
                let arr = r.as_array()?;
                Some((
                    arr.first()?.as_i64()? as i32,
                    arr.get(1)?.as_i64()? as i32,
                    arr.get(2)?.as_u64()? as u32,
                    arr.get(3)?.as_u64()? as u32,
                ))
            });

            let img_bytes = map_err!(p.screenshot(region), "screenshot failed: {0}")?;
            let decoded = image::load_from_memory(&img_bytes)
                .map_err(|e| ("COMPUTER_ERROR".into(), format!("image decode: {e}")))?;
            let items = map_err!(crate::ocr::ocr(&img_bytes), "ocr failed: {0}")?;

            let mut desc = format!(
                "Screen {}x{}\nDetected {} text elements:\n",
                decoded.width(),
                decoded.height(),
                items.len()
            );

            for item in &items {
                let cx = item.bounds.as_ref().map(|b| b.x + b.width / 2).unwrap_or(0);
                let cy = item
                    .bounds
                    .as_ref()
                    .map(|b| b.y + b.height / 2)
                    .unwrap_or(0);
                desc.push_str(&format!(
                    "  [{},{}] conf={:.2}: {}\n",
                    cx, cy, item.confidence, item.text
                ));
            }

            Ok(serde_json::json!({
                "description": desc,
                "width": decoded.width(),
                "height": decoded.height(),
                "elements": items.iter().map(|i| serde_json::json!({
                    "text": i.text,
                    "confidence": i.confidence,
                    "center": i.bounds.as_ref().map(|b| [b.x + b.width/2, b.y + b.height/2]),
                })).collect::<Vec<_>>(),
                "count": items.len(),
            }))
        }

        "wait" => {
            let secs = args["seconds"].as_f64().unwrap_or(1.0);
            tokio::time::sleep(Duration::from_secs_f64(secs)).await;
            Ok(serde_json::json!({"waited": secs}))
        }

        "clipboard_get" => {
            let text = map_err!(p.clipboard_get(), "clipboard_get failed: {0}")?;
            Ok(serde_json::json!({"text": text}))
        }

        "clipboard_set" => {
            let text = args["text"].as_str().unwrap_or("");
            map_err!(p.clipboard_set(text), "clipboard_set failed: {0}")?;
            Ok(serde_json::json!({}))
        }

        "env_get" => {
            let name = args["name"]
                .as_str()
                .ok_or(("INVALID_ARGS".into(), "Missing 'name' parameter".into()))?;
            let value = std::env::var(name).unwrap_or_default();
            Ok(serde_json::json!({
                "name": name,
                "value": value,
            }))
        }

        "shell_run" => {
            if std::env::var("ALLOW_SHELL").as_deref() != Ok("1") {
                return Err((
                    "SHELL_DISABLED".into(),
                    "shell_run requires ALLOW_SHELL=1 env var".into(),
                ));
            }
            let cmd = args["command"].as_str().unwrap_or("");
            let timeout = args.get("timeout").and_then(|v| v.as_u64()).unwrap_or(30);

            let result = map_err!(p.shell_run(cmd, timeout), "shell_run failed: {0}")?;

            Ok(serde_json::json!({
                "returncode": result.returncode,
                "stdout": result.stdout,
                "stderr": result.stderr,
            }))
        }

        "list_windows" => {
            let windows = map_err!(p.list_windows(), "list_windows failed: {0}")?;
            Ok(serde_json::json!({
                "windows": windows.iter().map(|w| serde_json::json!({
                    "id": w.id,
                    "title": w.title,
                    "app": w.app,
                    "pid": w.pid,
                    "geometry": {
                        "x": w.geometry.x,
                        "y": w.geometry.y,
                        "width": w.geometry.width,
                        "height": w.geometry.height,
                    }
                })).collect::<Vec<_>>(),
                "count": windows.len(),
            }))
        }

        "focus_window" => {
            let title = args["title"].as_str().unwrap_or("");
            let result = map_err!(p.focus_window(title), "focus_window failed: {0}")?;
            let json_result = serde_json::json!({
                "matched": result.matched,
                "id": result.id,
                "title": result.title,
                "app": result.app,
                "candidates": result.candidates,
            });
            with_screen(json_result).await
        }

        "get_active_window" => {
            let window = map_err!(p.get_active_window(), "get_active_window failed: {0}")?;
            match window {
                Some(w) => Ok(serde_json::json!({
                    "id": w.id, "title": w.title, "app": w.app,
                    "pid": w.pid,
                    "geometry": { "x": w.geometry.x, "y": w.geometry.y, "width": w.geometry.width, "height": w.geometry.height },
                })),
                None => Err(("NO_ACTIVE_WINDOW".into(), "no active window".into())),
            }
        }

        "open_app" => {
            let name = args["name"].as_str().or(args["app"].as_str()).unwrap_or("");
            map_err!(p.open_app(name), "open_app failed: {0}")?;
            let result = serde_json::json!({"launched": name});
            with_screen(result).await
        }

        "notify" => {
            let title = args["title"].as_str().unwrap_or("");
            let msg = args["message"].as_str().unwrap_or("");
            let urgency = args
                .get("urgency")
                .and_then(|v| v.as_str())
                .unwrap_or("normal");

            map_err!(p.notify(title, msg, urgency), "notify failed: {0}")?;
            Ok(serde_json::json!({}))
        }

        "get_window_state" => {
            // Optionally focus a window first
            if let Some(win_title) = args.get("window_title").and_then(|v| v.as_str()) {
                let fm = map_err!(p.focus_window(win_title), "focus_window failed: {0}")?;
                if !fm.matched {
                    // Don't fail — just warn. The tree may still be available.
                    eprintln!(
                        "WARN: window '{}' not found; using active window for element tree",
                        win_title
                    );
                }
            }

            let state = map_err!(p.get_window_state(), "get_window_state failed: {0}")?;
            Ok(serde_json::to_value(&state).map_err(|e| {
                (
                    format!("SERIALIZE_ERROR: {e}"),
                    format!("failed to serialize WindowState: {e}"),
                )
            })?)
        }

        "type_to_window" => {
            let title = args["title"].as_str().unwrap_or("");
            let text = args["text"].as_str().unwrap_or("");

            let result = map_err!(p.focus_window(title), "focus_window failed: {0}")?;

            if !result.matched {
                return Err((
                    "WINDOW_NOT_FOUND".into(),
                    format!("Window matching '{title}' not found"),
                ));
            }

            map_err!(p.keyboard_type(text, 10), "keyboard_type failed: {0}")?;
            let json_result = serde_json::json!({
                "matched": result.matched,
                "id": result.id,
                "title": result.title,
                "app": result.app,
            });
            with_screen(json_result).await
        }

        _ => Err(("UNKNOWN_TOOL".into(), format!("no computer tool '{name}'"))),
    }
}
