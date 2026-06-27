//! Computer use tool handlers — 24 tools for desktop control.

use crate::response::{self, ToolResponse};
use crate::PROVIDER;
use base64::Engine;
use serde_json::Value;
use std::time::Duration;

/// Handle all computer use tool calls
pub async fn handle(name: &str, args: Value) -> ToolResponse {
    let result = handle_inner(name, args).await;
    match result {
        Ok(value) => response::ok(value),
        Err(message) => response::err("COMPUTER_ERROR", &message),
    }
}

async fn handle_inner(name: &str, args: Value) -> Result<Value, String> {
    let p = &PROVIDER;

    macro_rules! map_err {
        ($expr:expr, $msg:literal) => {
            $expr.map_err(|e| format!($msg, e))
        };
    }

    match name {
        "screenshot" => {
            let region = args.get("region").and_then(|r| {
                let arr = r.as_array()?;
                Some((arr.get(0)?.as_i64()? as i32, arr.get(1)?.as_i64()? as i32,
                      arr.get(2)?.as_u64()? as u32, arr.get(3)?.as_u64()? as u32))
            });

            let img_bytes = map_err!(p.screenshot(region), "screenshot failed: {0}")?;

            let decoded = image::load_from_memory(&img_bytes)
                .map_err(|e| format!("image decode failed: {e}"))?;

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
            let smooth = args.get("smooth").and_then(|v| v.as_bool()).unwrap_or(false);
            let dur = args.get("duration_ms").and_then(|v| v.as_u64()).unwrap_or(200);

            map_err!(p.mouse_move(x, y, smooth, dur), "mouse_move failed: {0}")?;
            Ok(serde_json::json!({"moved_to": {"x": x, "y": y}}))
        }

        "mouse_click" => {
            let button = args.get("button").and_then(|v| v.as_str()).unwrap_or("left");
            let x = args.get("x").and_then(|v| v.as_i64()).map(|v| v as i32);
            let y = args.get("y").and_then(|v| v.as_i64()).map(|v| v as i32);
            let clicks = args.get("clicks").and_then(|v| v.as_u64()).unwrap_or(1) as u32;

            map_err!(p.mouse_click(button, x, y, clicks), "mouse_click failed: {0}")?;
            Ok(serde_json::json!({}))
        }

        "mouse_double_click" => {
            let button = args.get("button").and_then(|v| v.as_str()).unwrap_or("left");
            let x = args.get("x").and_then(|v| v.as_i64()).map(|v| v as i32);
            let y = args.get("y").and_then(|v| v.as_i64()).map(|v| v as i32);

            map_err!(p.mouse_click(button, x, y, 2), "mouse_double_click failed: {0}")?;
            Ok(serde_json::json!({}))
        }

        "mouse_scroll" => {
            let dx = args.get("dx").and_then(|v| v.as_i64()).unwrap_or(0) as i32;
            let dy = args.get("dy").and_then(|v| v.as_i64()).unwrap_or(0) as i32;
            let x = args.get("x").and_then(|v| v.as_i64()).map(|v| v as i32);
            let y = args.get("y").and_then(|v| v.as_i64()).map(|v| v as i32);

            map_err!(p.mouse_scroll(dx, dy, x, y), "mouse_scroll failed: {0}")?;
            Ok(serde_json::json!({}))
        }

        "mouse_drag" => {
            let x1 = args["x1"].as_i64().unwrap_or(0) as i32;
            let y1 = args["y1"].as_i64().unwrap_or(0) as i32;
            let x2 = args["x2"].as_i64().unwrap_or(0) as i32;
            let y2 = args["y2"].as_i64().unwrap_or(0) as i32;
            let button = args.get("button").and_then(|v| v.as_str()).unwrap_or("left");
            let dur = args.get("duration_ms").and_then(|v| v.as_u64()).unwrap_or(500);

            map_err!(p.mouse_drag(x1, y1, x2, y2, button, dur), "mouse_drag failed: {0}")?;
            Ok(serde_json::json!({}))
        }

        "keyboard_type" => {
            let text = args["text"].as_str().unwrap_or("");
            let delay = args.get("delay_ms").and_then(|v| v.as_u64()).unwrap_or(10);

            map_err!(p.keyboard_type(text, delay), "keyboard_type failed: {0}")?;
            Ok(serde_json::json!({"chars_typed": text.len()}))
        }

        "key_press" => {
            let key = args["key"].as_str().unwrap_or("");
            map_err!(p.key_press(key), "key_press failed: {0}")?;
            Ok(serde_json::json!({"key": key}))
        }

        "press_hotkey" => {
            let keys: Vec<String> = args["keys"]
                .as_array()
                .map(|a| a.iter().filter_map(|v| v.as_str().map(String::from)).collect())
                .unwrap_or_default();

            let combo = keys.join("+");
            map_err!(p.key_press(&combo), "press_hotkey failed: {0}")?;
            Ok(serde_json::json!({"combo": combo}))
        }

        "click_on_text" => {
            let text = args["text"].as_str().unwrap_or("").to_string();
            let button = args.get("button").and_then(|v| v.as_str()).unwrap_or("left");
            let partial = args.get("partial").and_then(|v| v.as_bool()).unwrap_or(true);

            let region = args.get("region").and_then(|r| {
                let arr = r.as_array()?;
                Some((arr.get(0)?.as_i64()? as i32, arr.get(1)?.as_i64()? as i32,
                      arr.get(2)?.as_u64()? as u32, arr.get(3)?.as_u64()? as u32))
            });

            let img_bytes = map_err!(p.screenshot(region), "screenshot failed: {0}")?;
            let items = map_err!(crate::ocr::ocr(&img_bytes), "ocr failed: {0}")?;

            let found = crate::ocr::find_text(&items, &text, partial)
                .ok_or_else(|| format!("Text '{text}' not found on screen"))?;

            let px = found.x + (found.width as i32) / 2;
            let py = found.y + (found.height as i32) / 2;

            map_err!(p.mouse_click(button, Some(px), Some(py), 1), "click failed: {0}")?;

            Ok(serde_json::json!({
                "text": found.text,
                "confidence": found.confidence,
                "clicked_at": [px, py],
            }))
        }

        "wait_for_text" => {
            let text = args["text"].as_str().unwrap_or("");
            let timeout = args.get("timeout").and_then(|v| v.as_f64()).unwrap_or(10.0);
            let partial = args.get("partial").and_then(|v| v.as_bool()).unwrap_or(true);

            let region = args.get("region").and_then(|r| {
                let arr = r.as_array()?;
                Some((arr.get(0)?.as_i64()? as i32, arr.get(1)?.as_i64()? as i32,
                      arr.get(2)?.as_u64()? as u32, arr.get(3)?.as_u64()? as u32))
            });

            let start = std::time::Instant::now();
            while start.elapsed().as_secs_f64() < timeout {
                let img_bytes = map_err!(p.screenshot(region), "screenshot failed: {0}")?;

                if let Ok(items) = crate::ocr::ocr(&img_bytes) {
                    if let Some(found) = crate::ocr::find_text(&items, text, partial) {
                        return Ok(serde_json::json!({
                            "text": found.text,
                            "confidence": found.confidence,
                            "position": [found.x + (found.width as i32)/2, found.y + (found.height as i32)/2],
                            "waited_ms": start.elapsed().as_millis(),
                        }));
                    }
                }
                tokio::time::sleep(Duration::from_millis(500)).await;
            }

            Err(format!("'{text}' not found within {timeout}s"))
        }

        "extract_text" => {
            let region = args.get("region").and_then(|r| {
                let arr = r.as_array()?;
                Some((arr.get(0)?.as_i64()? as i32, arr.get(1)?.as_i64()? as i32,
                      arr.get(2)?.as_u64()? as u32, arr.get(3)?.as_u64()? as u32))
            });

            let img_bytes = map_err!(p.screenshot(region), "screenshot failed: {0}")?;
            let items = map_err!(crate::ocr::ocr(&img_bytes), "ocr failed: {0}")?;

            Ok(serde_json::json!({
                "items": items.iter().map(|i| serde_json::json!({
                    "text": i.text,
                    "confidence": i.confidence,
                    "bbox": [i.x, i.y, i.width, i.height],
                })).collect::<Vec<_>>(),
                "count": items.len(),
            }))
        }

        "describe_screen" => {
            let region = args.get("region").and_then(|r| {
                let arr = r.as_array()?;
                Some((arr.get(0)?.as_i64()? as i32, arr.get(1)?.as_i64()? as i32,
                      arr.get(2)?.as_u64()? as u32, arr.get(3)?.as_u64()? as u32))
            });

            let img_bytes = map_err!(p.screenshot(region), "screenshot failed: {0}")?;
            let decoded = image::load_from_memory(&img_bytes)
                .map_err(|e| format!("image decode: {e}"))?;
            let items = map_err!(crate::ocr::ocr(&img_bytes), "ocr failed: {0}")?;

            let mut desc = format!("Screen {}x{}\nDetected {} text elements:\n",
                decoded.width(), decoded.height(), items.len());

            for item in &items {
                desc.push_str(&format!(
                    "  [{},{}] conf={:.2}: {}\n",
                    item.x + (item.width as i32) / 2,
                    item.y + (item.height as i32) / 2,
                    item.confidence,
                    item.text
                ));
            }

            Ok(serde_json::json!({
                "description": desc,
                "width": decoded.width(),
                "height": decoded.height(),
                "elements": items.iter().map(|i| serde_json::json!({
                    "text": i.text,
                    "confidence": i.confidence,
                    "center": [i.x + (i.width as i32)/2, i.y + (i.height as i32)/2],
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

        "shell_run" => {
            if std::env::var("ALLOW_SHELL").as_deref() != Ok("1") {
                return Err("shell_run requires ALLOW_SHELL=1 env var".to_string());
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
            Ok(serde_json::json!({
                "matched": result.matched,
                "id": result.id,
                "title": result.title,
                "app": result.app,
                "candidates": result.candidates,
            }))
        }

        "get_active_window" => {
            let window = map_err!(p.get_active_window(), "get_active_window failed: {0}")?;
            match window {
                Some(w) => Ok(serde_json::json!({
                    "id": w.id, "title": w.title, "app": w.app,
                    "geometry": { "x": w.geometry.x, "y": w.geometry.y, "width": w.geometry.width, "height": w.geometry.height },
                })),
                None => Err("no active window".to_string()),
            }
        }

        "open_app" => {
            let name = args["name"].as_str().unwrap_or("");
            map_err!(p.open_app(name), "open_app failed: {0}")?;
            Ok(serde_json::json!({"launched": name}))
        }

        "notify" => {
            let title = args["title"].as_str().unwrap_or("");
            let msg = args["message"].as_str().unwrap_or("");
            let urgency = args.get("urgency").and_then(|v| v.as_str()).unwrap_or("normal");

            map_err!(p.notify(title, msg, urgency), "notify failed: {0}")?;
            Ok(serde_json::json!({}))
        }

        "type_to_window" => {
            let title = args["title"].as_str().unwrap_or("");
            let text = args["text"].as_str().unwrap_or("");

            let result = map_err!(p.focus_window(title), "focus_window failed: {0}")?;

            if !result.matched {
                return Err(format!("Window matching '{title}' not found"));
            }

            map_err!(p.keyboard_type(text, 10), "keyboard_type failed: {0}")?;
            Ok(serde_json::json!({
                "matched": result.matched,
                "id": result.id,
                "title": result.title,
                "app": result.app,
            }))
        }

        _ => Err(format!("no computer tool '{name}'")),
    }
}
