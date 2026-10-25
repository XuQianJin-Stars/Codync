//! The models an agent offers, read from a session it opens for that alone.

use super::{probe_dir, start, text};
use crate::agent::acp::Incoming;
use crate::store::Store;
use anyhow::{Context, Result, bail};
use serde_json::{Value, json};
use std::time::Duration;

/// Discover the agent's advertised models in an isolated, prompt-free session.
/// This uses the same credentials as actual bot sessions and never reads project files.
pub async fn models(store: &Store, backend: &str) -> Result<Value> {
    let (acp, mut inbox, _, _) = start(store, backend).await?;
    let result = tokio::time::timeout(Duration::from_secs(60), async {
        let request = acp.request("session/new", json!({"cwd": probe_dir(), "mcpServers": []}));
        tokio::pin!(request);
        let session = loop {
            tokio::select! {
                result = &mut request => break result?,
                incoming = inbox.recv() => match incoming {
                    Some(Incoming::Request { id, .. }) => {
                        acp.respond_error(id, -32601, "not supported during model discovery").await?;
                    }
                    Some(Incoming::Notification { .. }) => {}
                    Some(Incoming::Closed { .. }) | None => bail!("agent closed during model discovery"),
                }
            }
        };
        // Cursor lists each model's options (effort, fast…) only through its own method.
        let options = if backend == "cursor" {
            acp.request("cursor/list_available_models", json!({})).await.unwrap_or_default()
        } else {
            Value::Null
        };
        Ok((session, options))
    })
    .await;
    acp.kill().await;
    let (session, options) = result.context("the agent's model list timed out")??;
    Ok(model_catalog(&session, &options))
}

/// IDs are agent-defined: use the category, with legacy conventional IDs as a fallback.
pub(crate) fn model_config(session: &Value) -> Option<&Value> {
    let options = session["configOptions"].as_array()?;
    let is_select = |v: &&Value| v["type"] == "select" || v.get("type").is_none();
    options
        .iter()
        .filter(is_select)
        .find(|v| v["category"] == "model")
        .or_else(|| options.iter().filter(is_select).find(|v| v["id"] == "model" || v["id"] == "models"))
}

/// The model ids the agent lists, in its order.
pub(crate) fn model_ids(session: &Value) -> Vec<String> {
    let catalog = model_catalog(session, &Value::Null);
    catalog["models"].as_array().into_iter().flatten().filter_map(|m| text(&m["id"])).collect()
}

/// `{models: [{id, name, description, options}], currentModelId, fastMode}`; `options` is
/// Cursor's `cursor/list_available_models` answer (`Null` for other agents).
fn model_catalog(session: &Value, options: &Value) -> Value {
    fn collect(options: &Value, out: &mut Vec<Value>) {
        for option in options.as_array().into_iter().flatten() {
            if option["options"].is_array() {
                collect(&option["options"], out);
            } else if let Some(id) = text(&option["value"])
                && !out.iter().any(|v| v["id"] == id)
            {
                out.push(json!({"id": id, "name": text(&option["name"]).unwrap_or(id),
                    "description": text(&option["description"])}));
            }
        }
    }
    let mut models = Vec::new();
    let current = if let Some(config) = model_config(session) {
        collect(&config["options"], &mut models);
        text(&config["currentValue"])
    } else {
        for model in session["models"]["availableModels"].as_array().into_iter().flatten() {
            if let Some(id) = text(&model["modelId"])
                && !models.iter().any(|v| v["id"] == id)
            {
                models.push(json!({"id": id, "name": text(&model["name"]).unwrap_or(id),
                    "description": text(&model["description"])}));
            }
        }
        text(&session["models"]["currentModelId"])
    };
    for model in &mut models {
        model["options"] = json!(model_options(options, model["id"].as_str().unwrap_or_default()));
    }
    json!({"models": models, "currentModelId": current,
        "fastMode": crate::agent::config::fast_mode_config(session).is_some()})
}

/// One model's options as `[{id, name, values: [{id, name}], default}]`, from Cursor's
/// `{models: [{value, configOptions}]}` (ACP select config options).
fn model_options(options: &Value, model: &str) -> Vec<Value> {
    let entry = options["models"].as_array().into_iter().flatten().find(|m| m["value"] == model);
    entry
        .and_then(|m| m["configOptions"].as_array())
        .into_iter()
        .flatten()
        .filter(|o| o["type"] == "select")
        .filter_map(|o| {
            let id = text(&o["id"])?;
            let values: Vec<Value> = o["options"]
                .as_array()?
                .iter()
                .filter_map(|v| {
                    let value = text(&v["value"])?;
                    Some(json!({"id": value, "name": text(&v["name"]).unwrap_or(value)}))
                })
                .collect();
            Some(json!({"id": id, "name": text(&o["name"]).unwrap_or(id), "values": values,
                "default": text(&o["currentValue"])}))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn model_catalog_reads_grouped_config_and_preserves_agent_ids() {
        let session = json!({"configOptions": [{
            "id": "provider-model", "category": "model", "type": "select", "currentValue": "m2",
            "options": [{"group": "a", "options": [
                {"value": "m1", "name": "First"}, {"value": "m2", "name": "Second"},
                {"value": "m1", "name": "Duplicate"}
            ]}]
        }], "models": {"availableModels": [{"modelId": "legacy", "name": "Legacy"}]}});
        assert_eq!(model_config(&session).unwrap()["id"], "provider-model");
        let result = model_catalog(&session, &Value::Null);
        assert_eq!(result["currentModelId"], "m2");
        assert_eq!(result["models"].as_array().unwrap().len(), 2);
        assert_eq!(result["models"][0]["id"], "m1");
        assert_eq!(result["models"][1]["name"], "Second");
        assert_eq!(result["models"][1]["options"], json!([]));
        assert_eq!(model_ids(&session), ["m1", "m2"]);
    }

    #[test]
    fn model_catalog_reads_legacy_and_missing_lists() {
        let result = model_catalog(
            &json!({"models": {
                "currentModelId": "a", "availableModels": [
                    {"modelId": "a", "name": "Agent model", "description": "Available here"},
                    {"name": "No ID"}
                ]
            }}),
            &Value::Null,
        );
        assert_eq!(result["models"].as_array().unwrap().len(), 1);
        assert_eq!(result["models"][0]["description"], "Available here");
        assert_eq!(
            model_catalog(&json!({}), &Value::Null),
            json!({"models": [], "currentModelId": null, "fastMode": false})
        );
    }

    #[test]
    fn model_catalog_adds_cursor_model_options() {
        let session = json!({"configOptions": [{"id": "model", "category": "model", "type": "select",
            "currentValue": "grok-4.7", "options": [{"value": "grok-4.7", "name": "Grok 4.7"}, {"value": "auto"}]}]});
        let listed = json!({"models": [{"value": "grok-4.7", "name": "Grok 4.7", "configOptions": [
            {"id": "reasoning_effort", "name": "Reasoning", "category": "thought_level", "type": "select",
             "currentValue": "high", "options": [{"value": "medium", "name": "Medium"}, {"value": "high", "name": "High"}]},
            {"id": "fast", "type": "select", "currentValue": "true", "options": [{"value": "false", "name": "Off"}, {"value": "true"}]},
            {"id": "verbose", "type": "boolean"}
        ]}]});
        let result = model_catalog(&session, &listed);
        assert_eq!(
            result["models"][0]["options"],
            json!([
                {"id": "reasoning_effort", "name": "Reasoning", "default": "high",
                 "values": [{"id": "medium", "name": "Medium"}, {"id": "high", "name": "High"}]},
                {"id": "fast", "name": "fast", "default": "true",
                 "values": [{"id": "false", "name": "Off"}, {"id": "true", "name": "true"}]}
            ])
        );
        assert_eq!(result["models"][1]["options"], json!([]));
    }

    #[test]
    fn model_config_prefers_category_and_ignores_unknown_types() {
        let session = json!({"configOptions": [
            {"id": "model", "type": "boolean"},
            {"id": "models", "type": "select"},
            {"id": "engine", "category": "model", "type": "select"}
        ]});
        assert_eq!(model_config(&session).unwrap()["id"], "engine");
        assert!(model_config(&json!({"configOptions": [{"id": "model", "type": "boolean"}]})).is_none());
    }
}
