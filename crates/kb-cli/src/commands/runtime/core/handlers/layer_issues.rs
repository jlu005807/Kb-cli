
const LAYER_L0_DECLARATION: &str = "l0_declaration";
const LAYER_L1_REGISTRY: &str = "l1_registry";
const LAYER_L1_INDEX: &str = "l1_index";
const LAYER_L2_SEMANTIC: &str = "l2_semantic";
const LAYER_L3_PROJECTION: &str = "l3_projection";
const LAYER_L4_GRAPH: &str = "l4_graph";
const LAYER_L5_WIKI_VIEW: &str = "l5_wiki_view";
const LAYER_L3_L4_PROJECTION_LEGACY: &str = "l3_l4_projection";
const UPDATE_LAYER_FAILURES_ENV: &str = "KB_UPDATE_SIMULATE_LAYER_FAILURES";

#[derive(Debug, Clone)]
struct LayerIssue {
    layer: &'static str,
    code: String,
    message: String,
    retryable: bool,
}

fn parse_simulated_layer_failures() -> HashSet<String> {
    std::env::var(UPDATE_LAYER_FAILURES_ENV)
        .ok()
        .map(|raw| {
            raw.split(',')
                .map(|part| part.trim().to_ascii_lowercase())
                .filter(|part| !part.is_empty())
                .collect::<HashSet<_>>()
        })
        .unwrap_or_default()
}

fn collect_progressive_layer_issues() -> Vec<LayerIssue> {
    let injected = parse_simulated_layer_failures();
    let mut issues = Vec::new();
    let legacy_l3_l4 = injected.contains(LAYER_L3_L4_PROJECTION_LEGACY);

    if injected.contains(LAYER_L2_SEMANTIC) {
        issues.push(LayerIssue {
            layer: LAYER_L2_SEMANTIC,
            code: "W-L2-EXTRACT-FAILED".to_string(),
            message: "Semantic extraction stage failed".to_string(),
            retryable: true,
        });
    }
    if injected.contains(LAYER_L3_PROJECTION) || legacy_l3_l4 {
        issues.push(LayerIssue {
            layer: LAYER_L3_PROJECTION,
            code: "W-L3-PROJECTION-FAILED".to_string(),
            message: "Search projection stage failed".to_string(),
            retryable: true,
        });
    }
    if injected.contains(LAYER_L4_GRAPH) || legacy_l3_l4 {
        issues.push(LayerIssue {
            layer: LAYER_L4_GRAPH,
            code: "W-L4-GRAPH-FAILED".to_string(),
            message: "Graph build stage failed".to_string(),
            retryable: true,
        });
    }
    if injected.contains(LAYER_L5_WIKI_VIEW) {
        issues.push(LayerIssue {
            layer: LAYER_L5_WIKI_VIEW,
            code: "W-L5-WIKI-VIEW-FAILED".to_string(),
            message: "Wiki markdown view stage failed".to_string(),
            retryable: true,
        });
    }
    issues
}

fn issue_json(issue: &LayerIssue) -> Value {
    json!({
        "layer": issue.layer,
        "code": issue.code,
        "message": issue.message,
        "retryable": issue.retryable,
    })
}

fn issue_text(issue: &LayerIssue) -> String {
    format!(
        "[{}] {}: {} ({})",
        issue.layer,
        issue.code,
        issue.message,
        if issue.retryable {
            "retryable"
        } else {
            "non-retryable"
        }
    )
}

fn generated_at_unix_string() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    format!("unix:{}", secs)
}
