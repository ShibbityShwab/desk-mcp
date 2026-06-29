//! Tool Recipes — reusable, parameterised sequences of MCP tool calls.
//!
//! Recipes are JSON files stored in `~/.config/desk-mcp/recipes/`.
//! Each recipe defines a named workflow with typed parameters and a list
//! of tool steps.  Parameters in step values use `{param_name}` placeholder
//! syntax and are substituted at call time.
//!
//! ## Example recipe
//!
//! ```json
//! {
//!   "name": "fill_github_issue",
//!   "description": "Navigate to GitHub and create a new issue",
//!   "version": "1.0.0",
//!   "parameters": {
//!     "repo": {"type": "string", "required": true},
//!     "title": {"type": "string", "required": true},
//!     "body": {"type": "string", "required": true}
//!   },
//!   "steps": [
//!     {"tool": "browser_navigate", "params": {"url": "https://github.com/{repo}/issues/new"}},
//!     {"tool": "browser_type", "params": {"selector": "#issue_title", "text": "{title}"}},
//!     {"tool": "browser_type", "params": {"selector": "#issue_body", "text": "{body}"}},
//!     {"tool": "browser_click", "params": {"text": "Submit new issue"}}
//!   ]
//! }
//! ```

use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

// ── Data types ──────────────────────────────────────────────────────────

/// A named, parameterised sequence of MCP tool calls.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Recipe {
    pub name: String,
    pub description: String,
    pub version: String,
    #[serde(default)]
    pub parameters: HashMap<String, RecipeParam>,
    pub steps: Vec<RecipeStep>,
}

/// Metadata for a single recipe parameter.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecipeParam {
    #[serde(rename = "type")]
    pub param_type: String,
    #[serde(default)]
    pub required: bool,
    #[serde(default)]
    pub default: Option<serde_json::Value>,
}

/// A single step inside a recipe — a tool name plus its (possibly templated) params.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecipeStep {
    pub tool: String,
    #[serde(default)]
    pub params: serde_json::Value,
}

// ── Recipe directory ────────────────────────────────────────────────────

fn recipes_dir() -> PathBuf {
    let mut p = dirs_next::config_dir().unwrap_or_else(|| PathBuf::from("/tmp"));
    p.push("desk-mcp");
    p.push("recipes");
    p
}

// ── Loading ─────────────────────────────────────────────────────────────

/// Load all recipes from `~/.config/desk-mcp/recipes/*.json`.
pub fn load_all_recipes() -> Vec<Recipe> {
    let dir = recipes_dir();
    if !dir.exists() {
        return vec![];
    }

    let entries = match fs::read_dir(&dir) {
        Ok(e) => e,
        Err(e) => {
            tracing::warn!(dir = %dir.display(), error = %e, "cannot read recipes directory");
            return vec![];
        }
    };

    let mut recipes = Vec::new();

    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) != Some("json") {
            continue;
        }

        match fs::read_to_string(&path) {
            Ok(text) => match serde_json::from_str::<Recipe>(&text) {
                Ok(recipe) => {
                    tracing::debug!(recipe = %recipe.name, path = %path.display(), "loaded recipe");
                    recipes.push(recipe);
                }
                Err(e) => {
                    tracing::warn!(path = %path.display(), error = %e, "failed to parse recipe");
                }
            },
            Err(e) => {
                tracing::warn!(path = %path.display(), error = %e, "failed to read recipe file");
            }
        }
    }

    recipes
}

/// Find a recipe by exact name.
pub fn find_recipe(name: &str) -> Option<Recipe> {
    // First, try the on-disk directory (most up-to-date)
    let dir = recipes_dir();
    if dir.exists() {
        let path = dir.join(format!("{name}.json"));
        if path.exists() {
            if let Ok(text) = fs::read_to_string(&path) {
                if let Ok(recipe) = serde_json::from_str::<Recipe>(&text) {
                    return Some(recipe);
                }
            }
        }
    }

    // Fall back to the pre-loaded cache
    load_all_recipes().into_iter().find(|r| r.name == name)
}

// ── Parameter substitution ──────────────────────────────────────────────

/// Recursively walk a JSON value and replace `{key}` placeholders with
/// values from `params`.
///
/// Substitution rules:
/// - String nodes: `{param_name}` → value from `params` if present (string interpolation).
///   The placeholder must match the entire string: `"Hello {name}"` does *not* interpolate;
///   use `"{greeting} {name}"` by passing the whole string as one param.
/// - Object nodes: recursively process each value.
/// - Array nodes: recursively process each element.
/// - Other scalars (numbers, bools, null): returned unchanged.
pub fn substitute_params(
    template: &serde_json::Value,
    params: &HashMap<String, String>,
) -> serde_json::Value {
    match template {
        serde_json::Value::String(s) => {
            // Check for exact `{key}` match
            if s.starts_with('{') && s.ends_with('}') && s.len() > 2 {
                let key = &s[1..s.len() - 1];
                if let Some(val) = params.get(key) {
                    return serde_json::Value::String(val.clone());
                }
            }
            // Partial interpolation: replace each `{key}` occurrence
            let mut result = s.clone();
            for (key, val) in params {
                let placeholder = format!("{{{key}}}");
                result = result.replace(&placeholder, val);
            }
            serde_json::Value::String(result)
        }
        serde_json::Value::Object(map) => {
            let mut out = serde_json::Map::new();
            for (k, v) in map {
                out.insert(k.clone(), substitute_params(v, params));
            }
            serde_json::Value::Object(out)
        }
        serde_json::Value::Array(arr) => {
            let out: Vec<serde_json::Value> = arr
                .iter()
                .map(|v| substitute_params(v, params))
                .collect();
            serde_json::Value::Array(out)
        }
        other => other.clone(),
    }
}

// ── Recipe → synthetic ToolDef ─────────────────────────────────────────

/// Build a JSON Schema for the recipe's parameters so it can be listed
/// alongside built-in tools.
pub fn recipe_input_schema(recipe: &Recipe) -> serde_json::Value {
    let mut properties = serde_json::Map::new();
    let mut required: Vec<serde_json::Value> = Vec::new();

    for (name, param) in &recipe.parameters {
        let mut prop = serde_json::Map::new();
        prop.insert("type".into(), serde_json::Value::String(param.param_type.clone()));
        if let Some(ref default) = param.default {
            prop.insert("default".into(), default.clone());
        }
        properties.insert(name.clone(), serde_json::Value::Object(prop));
        if param.required {
            required.push(serde_json::Value::String(name.clone()));
        }
    }

    let mut schema = serde_json::Map::new();
    schema.insert("type".into(), serde_json::Value::String("object".into()));
    schema.insert(
        "properties".into(),
        serde_json::Value::Object(properties),
    );
    if !required.is_empty() {
        schema.insert("required".into(), serde_json::Value::Array(required));
    }

    serde_json::Value::Object(schema)
}

// ── Tests ───────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_recipe() -> Recipe {
        Recipe {
            name: "test_recipe".into(),
            description: "A test".into(),
            version: "1.0.0".into(),
            parameters: {
                let mut m = HashMap::new();
                m.insert(
                    "url".into(),
                    RecipeParam {
                        param_type: "string".into(),
                        required: true,
                        default: None,
                    },
                );
                m.insert(
                    "count".into(),
                    RecipeParam {
                        param_type: "integer".into(),
                        required: false,
                        default: Some(serde_json::json!(1)),
                    },
                );
                m
            },
            steps: vec![
                RecipeStep {
                    tool: "browser_navigate".into(),
                    params: serde_json::json!({"url": "{url}"}),
                },
                RecipeStep {
                    tool: "wait".into(),
                    params: serde_json::json!({"seconds": "{count}"}),
                },
            ],
        }
    }

    #[test]
    fn substitute_exact_match() {
        let template = serde_json::json!("{url}");
        let mut params = HashMap::new();
        params.insert("url".into(), "https://example.com".into());
        let result = substitute_params(&template, &params);
        assert_eq!(result, serde_json::json!("https://example.com"));
    }

    #[test]
    fn substitute_partial_in_string() {
        let template = serde_json::json!("Visit {url} now");
        let mut params = HashMap::new();
        params.insert("url".into(), "https://example.com".into());
        let result = substitute_params(&template, &params);
        assert_eq!(result, serde_json::json!("Visit https://example.com now"));
    }

    #[test]
    fn substitute_in_object() {
        let template = serde_json::json!({"url": "{url}", "count": "{count}"});
        let mut params = HashMap::new();
        params.insert("url".into(), "https://example.com".into());
        params.insert("count".into(), "5".into());
        let result = substitute_params(&template, &params);
        assert_eq!(
            result,
            serde_json::json!({"url": "https://example.com", "count": "5"})
        );
    }

    #[test]
    fn substitute_in_array() {
        let template = serde_json::json!(["{a}", "{b}"]);
        let mut params = HashMap::new();
        params.insert("a".into(), "hello".into());
        params.insert("b".into(), "world".into());
        let result = substitute_params(&template, &params);
        assert_eq!(result, serde_json::json!(["hello", "world"]));
    }

    #[test]
    fn substitute_missing_key_unchanged() {
        let template = serde_json::json!("{missing}");
        let params = HashMap::new();
        let result = substitute_params(&template, &params);
        assert_eq!(result, serde_json::json!("{missing}"));
    }

    #[test]
    fn substitute_non_strings_unchanged() {
        assert_eq!(
            substitute_params(&serde_json::json!(42), &HashMap::new()),
            serde_json::json!(42)
        );
        assert_eq!(
            substitute_params(&serde_json::json!(true), &HashMap::new()),
            serde_json::json!(true)
        );
        assert_eq!(
            substitute_params(&serde_json::json!(null), &HashMap::new()),
            serde_json::json!(null)
        );
    }

    #[test]
    fn recipe_input_schema_generates_valid_json() {
        let recipe = sample_recipe();
        let schema = recipe_input_schema(&recipe);
        let schema_str = serde_json::to_string(&schema).unwrap();
        assert!(schema_str.contains("\"type\":\"object\""));
        assert!(schema_str.contains("\"url\""));
        assert!(schema_str.contains("\"count\""));
        // url is required
        assert!(schema_str.contains("\"required\""));
    }

    #[test]
    fn load_all_recipes_empty_when_dir_missing() {
        let recipes = load_all_recipes();
        // In test/CI the recipes directory typically does not exist
        assert!(recipes.is_empty());
    }

    #[test]
    fn find_recipe_returns_none_for_missing() {
        assert!(find_recipe("nonexistent_recipe_xyz").is_none());
    }
}
