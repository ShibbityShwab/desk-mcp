//! Web search tool — DuckDuckGo HTML (no API key required).
//!
//! Exposes a single tool: `web_search(query)` → list of results with
//! title, URL, and snippet.

use crate::response;
use crate::tools::ToolResponse;

/// DuckDuckGo HTML search result scraped from the "links" page.
#[derive(Debug, serde::Serialize)]
pub struct SearchResult {
    pub title: String,
    pub url: String,
    pub snippet: String,
}

pub async fn web_search(query: &str, max_results: usize) -> Result<Vec<SearchResult>, String> {
    let url = format!(
        "https://html.duckduckgo.com/html/?q={}",
        urlencoding::encode(query)
    );

    let client = reqwest::Client::builder()
        .user_agent("Mozilla/5.0 (compatible; desk-mcp/1.0)")
        .timeout(std::time::Duration::from_secs(15))
        .build()
        .map_err(|e| format!("reqwest: {e}"))?;

    let resp = client
        .get(&url)
        .send()
        .await
        .map_err(|e| format!("fetch: {e}"))?;
    let body = resp.text().await.map_err(|e| format!("read: {e}"))?;

    parse_duckduckgo_html(&body, max_results)
}

fn parse_duckduckgo_html(html: &str, max: usize) -> Result<Vec<SearchResult>, String> {
    let mut results = Vec::new();

    // DuckDuckGo HTML results are in <a class="result__a"> for titles
    // and <a class="result__snippet"> for snippets.
    // Simple substring-based parser — avoids full HTML dependency.

    let mut rest = html;
    while results.len() < max {
        // Find next result link
        let link_start = match rest.find("class=\"result__a\"") {
            Some(i) => i,
            None => break,
        };

        // Find href in the <a> tag containing result__a
        let tag_start = rest[..link_start].rfind("<a").unwrap_or(link_start);
        let href_start = match rest[tag_start..].find("href=\"") {
            Some(i) => tag_start + i + 6,
            None => {
                rest = &rest[link_start + 16..];
                continue;
            }
        };
        let href_end = match rest[href_start..].find('"') {
            Some(i) => href_start + i,
            None => {
                rest = &rest[link_start + 16..];
                continue;
            }
        };
        let raw_url = &rest[href_start..href_end];

        // Decode DuckDuckGo redirect URLs to their real destination.
        // Format: //duckduckgo.com/l/?uddg=<url-encoded-real-url>
        let url = if raw_url.contains("duckduckgo.com/l/?uddg=") {
            if let Some(encoded) = raw_url.split("uddg=").nth(1) {
                let encoded = encoded.split('&').next().unwrap_or(encoded);
                urlencoding::decode(encoded)
                    .unwrap_or_else(|_| raw_url.into())
                    .into_owned()
            } else {
                raw_url.to_string()
            }
        } else {
            raw_url.to_string()
        };

        // Find the link text (inside the <a>)
        let tag_close = rest[href_end..].find('>').unwrap_or(0) + href_end;
        let text_end = rest[tag_close + 1..].find("</a>").unwrap_or(0) + tag_close + 1;
        let title = rest[tag_close + 1..text_end].trim().to_string();

        // Find the snippet
        let snippet_start = match rest[link_start..].find("class=\"result__snippet\"") {
            Some(i) => link_start + i,
            None => {
                rest = &rest[link_start + 16..];
                continue;
            }
        };
        let snip_open = rest[snippet_start..].find('>').unwrap_or(0) + snippet_start;
        let snip_close = rest[snip_open + 1..].find("</").unwrap_or(0) + snip_open + 1;
        let snippet = rest[snip_open + 1..snip_close].trim().to_string();

        results.push(SearchResult {
            title,
            url,
            snippet,
        });

        rest = &rest[snip_close + 4..];
    }

    if results.is_empty() {
        return Err("no results found".into());
    }

    Ok(results)
}

/// Handle `web_fetch` tool invocation.
pub async fn handle_fetch(args: &serde_json::Value) -> ToolResponse {
    let url = args["url"].as_str().unwrap_or("");
    if url.is_empty() {
        return response::err("web_fetch", "url parameter is required");
    }

    if !url.starts_with("http://") && !url.starts_with("https://") {
        return response::err("web_fetch", "url must start with http:// or https://");
    }

    let format = args["format"].as_str().unwrap_or("text");
    let max_bytes = args
        .get("max_bytes")
        .and_then(|v| v.as_u64())
        .unwrap_or(500_000)
        .min(5_000_000) as usize;

    let client = reqwest::Client::builder()
        .user_agent("Mozilla/5.0 (compatible; desk-mcp/1.0)")
        .timeout(std::time::Duration::from_secs(20))
        .build()
        .map_err(|e| format!("reqwest build: {e}"));

    let client = match client {
        Ok(c) => c,
        Err(e) => return response::err("web_fetch", &e),
    };

    let resp = match client.get(url).send().await {
        Ok(r) => r,
        Err(e) => return response::err("web_fetch", &format!("HTTP request failed: {e}")),
    };

    let status = resp.status().as_u16();
    let content_type = resp
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("unknown")
        .to_string();

    // Read up to max_bytes
    let bytes = match resp.bytes().await {
        Ok(b) => b,
        Err(e) => return response::err("web_fetch", &format!("Failed to read response body: {e}")),
    };

    let truncated = bytes.len() > max_bytes;
    let body_bytes = if truncated {
        &bytes[..max_bytes]
    } else {
        &bytes
    };

    let body_str = String::from_utf8_lossy(body_bytes).to_string();

    let content = if format == "html" {
        body_str
    } else {
        // HTML-to-text: strip <script>, <style>, remaining <tags>, and entities.
        let lower = body_str.to_lowercase();
        let len = body_str.len();
        let chars: Vec<char> = body_str.chars().collect();
        let mut out = String::with_capacity(len);
        let mut i = 0;

        while i < len {
            if chars[i] == '<' {
                // Check for script/style — skip entire block until closing tag
                let rest_start = i + 1;
                let rest_end = (rest_start + 10).min(len);
                let rest: String = chars[rest_start..rest_end].iter().collect();
                let rest_lower = rest.to_lowercase();

                if rest_lower.starts_with("script") {
                    // Skip until </script>
                    if let Some(end) = lower[i..].find("</script>") {
                        i += end + 9; // skip past </script>
                        continue;
                    }
                } else if rest_lower.starts_with("style") {
                    if let Some(end) = lower[i..].find("</style>") {
                        i += end + 8;
                        continue;
                    }
                } else if rest_lower.starts_with("/script") || rest_lower.starts_with("/style") {
                    // Closing tag — skip to >
                    if let Some(end) = body_str[i..].find('>') {
                        i += end + 1;
                        continue;
                    }
                }
                // Regular tag — skip to >
                if let Some(end) = body_str[i..].find('>') {
                    i += end + 1;
                } else {
                    i += 1;
                }
                continue;
            }
            out.push(chars[i]);
            i += 1;
        }

        // Decode common HTML entities
        let decoded = out
            .replace("&amp;", "&")
            .replace("&lt;", "<")
            .replace("&gt;", ">")
            .replace("&quot;", "\"")
            .replace("&#39;", "'")
            .replace("&nbsp;", " ");

        // Collapse whitespace
        decoded
            .lines()
            .map(|l| l.trim())
            .filter(|l| !l.is_empty())
            .collect::<Vec<_>>()
            .join("\n")
    };

    response::ok(serde_json::json!({
        "url": url,
        "status": status,
        "content_type": content_type,
        "size": bytes.len(),
        "truncated": truncated,
        "content": content,
    }))
}

/// Handle `web_search` tool invocation.
pub async fn handle(args: &serde_json::Value) -> ToolResponse {
    let query = args["query"].as_str().unwrap_or("");
    if query.is_empty() {
        return response::err("web_search", "query parameter is required");
    }

    let max = args
        .get("max_results")
        .and_then(|v| v.as_u64())
        .unwrap_or(5)
        .min(10) as usize;

    match web_search(query, max).await {
        Ok(results) => {
            let out: Vec<serde_json::Value> = results
                .iter()
                .map(|r| {
                    serde_json::json!({
                        "title": r.title,
                        "url": r.url,
                        "snippet": r.snippet,
                    })
                })
                .collect();
            response::ok(serde_json::json!({
                "query": query,
                "results": out,
                "count": out.len(),
            }))
        }
        Err(e) => response::err("web_search", &e),
    }
}
