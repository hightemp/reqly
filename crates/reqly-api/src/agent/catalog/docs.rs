//! `get_docs`: the built-in help (English) for agents, so they learn the
//! template syntax, scripts and the rest without looking for Reqly's code.

use reqly_core::CoreResult;
use serde::Deserialize;
use serde_json::{Value, json};

use super::{args, invalid, tool};
use crate::agent::{Annotations, Tool};

macro_rules! help_dir {
    () => {
        concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../ui/src/features/help/content/en"
        )
    };
}

/// Help pages: topic id and Markdown (the same pages as the app's help).
const PAGES: &[(&str, &str)] = &[
    ("ai", include_str!(concat!(help_dir!(), "/ai.md"))),
    ("auth", include_str!(concat!(help_dir!(), "/auth.md"))),
    (
        "collaboration",
        include_str!(concat!(help_dir!(), "/collaboration.md")),
    ),
    (
        "getting-started",
        include_str!(concat!(help_dir!(), "/getting-started.md")),
    ),
    (
        "import-export",
        include_str!(concat!(help_dir!(), "/import-export.md")),
    ),
    ("mcp", include_str!(concat!(help_dir!(), "/mcp.md"))),
    (
        "protocols",
        include_str!(concat!(help_dir!(), "/protocols.md")),
    ),
    (
        "requests",
        include_str!(concat!(help_dir!(), "/requests.md")),
    ),
    (
        "script-api",
        include_str!(concat!(help_dir!(), "/script-api.md")),
    ),
    (
        "script-libraries",
        include_str!(concat!(help_dir!(), "/script-libraries.md")),
    ),
    ("scripts", include_str!(concat!(help_dir!(), "/scripts.md"))),
    (
        "settings",
        include_str!(concat!(help_dir!(), "/settings.md")),
    ),
    (
        "shortcuts",
        include_str!(concat!(help_dir!(), "/shortcuts.md")),
    ),
    ("sync", include_str!(concat!(help_dir!(), "/sync.md"))),
    ("testing", include_str!(concat!(help_dir!(), "/testing.md"))),
    (
        "variables",
        include_str!(concat!(help_dir!(), "/variables.md")),
    ),
];

pub fn tools() -> Vec<Tool> {
    vec![tool(
        "get_docs",
        "Reqly's documentation (the app's built-in help). Without `topic`: the list of topics. \
         With `topic`: that page as Markdown. Read `variables` for the template syntax \
         ({{var}}, {% tags %}, response chaining), `scripts` and `script-api` for scripts, \
         `auth` for authentication.",
        json!({
            "type": "object",
            "properties": {
                "topic": { "type": "string", "description": "Topic id from the list, e.g. variables." }
            }
        }),
        Annotations::read_only(),
        |_, a| Box::pin(async move { get_docs(args(a)?) }),
    )]
}

#[derive(Deserialize)]
struct DocsArgs {
    topic: Option<String>,
}

fn title(markdown: &str) -> &str {
    markdown
        .lines()
        .find_map(|l| l.strip_prefix("# "))
        .unwrap_or("")
        .trim()
}

fn get_docs(a: DocsArgs) -> CoreResult<Value> {
    let Some(topic) = a.topic.filter(|t| !t.trim().is_empty()) else {
        let topics: Vec<Value> = PAGES
            .iter()
            .map(|(id, md)| json!({ "id": id, "title": title(md) }))
            .collect();
        return Ok(json!({ "topics": topics }));
    };
    let topic = topic.trim().trim_end_matches(".md").to_lowercase();
    match PAGES.iter().find(|(id, _)| *id == topic) {
        Some((id, md)) => Ok(json!({ "topic": id, "title": title(md), "markdown": md })),
        None => {
            let ids: Vec<&str> = PAGES.iter().map(|(id, _)| *id).collect();
            Err(invalid(&format!(
                "unknown topic {topic:?}; topics: {}",
                ids.join(", ")
            )))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::PAGES;

    #[test]
    fn every_help_page_is_listed() {
        let dir = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../ui/src/features/help/content/en"
        );
        let mut files: Vec<String> = std::fs::read_dir(dir)
            .unwrap()
            .filter_map(|e| {
                let name = e.unwrap().file_name().into_string().unwrap();
                name.strip_suffix(".md").map(str::to_string)
            })
            .collect();
        files.sort();
        let listed: Vec<&str> = PAGES.iter().map(|(id, _)| *id).collect();
        assert_eq!(files, listed, "add new help pages to PAGES");
    }
}
