use std::collections::HashSet;

use crate::{types, utils::helper};

use types::{DepsArgs, DepKind, FnGraphArgs, FnNodeInfo, GraphData, CallKind, FnGraphData, Theme};
use helper::{format_node_label, sanitize_name};

// ============================================================================
// Output Generators
// ============================================================================

pub fn generate_deps_mermaid(graph_data: &GraphData, args: &DepsArgs) -> String {
    let mut output = String::new();

    if !args.no_fence {
        output.push_str("```mermaid\n");
    }

    output.push_str(&format!("flowchart {}\n", args.direction));

    // Theme styling
    match args.theme {
        Theme::Dark => {
            output.push_str("    %%{init: {'theme': 'dark'}}%%\n");
        }
        Theme::Light => {
            output.push_str("    %%{init: {'theme': 'default'}}%%\n");
        }
        Theme::Default => {}
    }

    // Collect edges by kind for grouping
    let mut normal_edges: Vec<(String, String)> = Vec::new();
    let mut dev_edges: Vec<(String, String)> = Vec::new();
    let mut build_edges: Vec<(String, String)> = Vec::new();

    for edge in graph_data.graph.edge_indices() {
        if let Some((from, to)) = graph_data.graph.edge_endpoints(edge) {
            let from_info = &graph_data.graph[from];
            let to_info = &graph_data.graph[to];
            let edge_kind = graph_data.graph[edge];

            let from_label = format_node_label(from_info, args);
            let to_label = format_node_label(to_info, args);

            match edge_kind {
                DepKind::Dev => dev_edges.push((from_label, to_label)),
                DepKind::Build => build_edges.push((from_label, to_label)),
                DepKind::Normal => normal_edges.push((from_label, to_label)),
            }
        }
    }

    if args.group_by_kind {
        // Grouped output
        if !normal_edges.is_empty() {
            output.push_str("    subgraph normal[\"Dependencies\"]\n");
            for (from, to) in &normal_edges {
                output.push_str(&format!("        {} --> {}\n", from, to));
            }
            output.push_str("    end\n");
        }
        if !dev_edges.is_empty() {
            output.push_str("    subgraph dev[\"Dev Dependencies\"]\n");
            for (from, to) in &dev_edges {
                output.push_str(&format!("        {} -.-> {}\n", from, to));
            }
            output.push_str("    end\n");
        }
        if !build_edges.is_empty() {
            output.push_str("    subgraph build[\"Build Dependencies\"]\n");
            for (from, to) in &build_edges {
                output.push_str(&format!("        {} ==> {}\n", from, to));
            }
            output.push_str("    end\n");
        }
    } else {
        // Flat output with different arrow styles
        for (from, to) in &normal_edges {
            output.push_str(&format!("    {} --> {}\n", from, to));
        }
        for (from, to) in &dev_edges {
            output.push_str(&format!("    {} -.-> {}\n", from, to));
        }
        for (from, to) in &build_edges {
            output.push_str(&format!("    {} ==> {}\n", from, to));
        }
    }

    // Highlight styling
    for highlight in &args.highlight {
        let sanitized = sanitize_name(highlight);
        output.push_str(&format!("    style {} fill:#f9f,stroke:#333,stroke-width:4px\n", sanitized));
    }

    if !args.no_fence {
        output.push_str("```\n");
    }

    output
}

pub fn generate_deps_dot(graph_data: &GraphData, args: &DepsArgs) -> String {
    let mut output = String::new();

    output.push_str("digraph dependencies {\n");
    output.push_str("    rankdir=LR;\n");
    output.push_str("    node [shape=box, style=rounded];\n");

    // Theme
    match args.theme {
        Theme::Dark => {
            output.push_str("    bgcolor=\"#1e1e1e\";\n");
            output.push_str("    node [fontcolor=white, color=white];\n");
            output.push_str("    edge [color=white];\n");
        }
        Theme::Light => {
            output.push_str("    bgcolor=white;\n");
        }
        Theme::Default => {}
    }

    // Node definitions
    let mut defined_nodes: HashSet<String> = HashSet::new();
    for idx in graph_data.graph.node_indices() {
        let info = &graph_data.graph[idx];
        let label = format_node_label(info, args);
        let sanitized = sanitize_name(&info.name);

        if defined_nodes.insert(sanitized.clone()) {
            let mut node_attrs = vec![format!("label=\"{}\"", label.replace('_', "-"))];

            if args.highlight.contains(&info.name) {
                node_attrs.push("fillcolor=\"#ff99ff\"".to_string());
                node_attrs.push("style=\"filled,rounded\"".to_string());
            }

            if info.is_workspace_member {
                node_attrs.push("penwidth=2".to_string());
            }

            output.push_str(&format!("    {} [{}];\n", sanitized, node_attrs.join(", ")));
        }
    }

    // Edges
    for edge in graph_data.graph.edge_indices() {
        if let Some((from, to)) = graph_data.graph.edge_endpoints(edge) {
            let from_name = sanitize_name(&graph_data.graph[from].name);
            let to_name = sanitize_name(&graph_data.graph[to].name);
            let kind = graph_data.graph[edge];

            let style = match kind {
                DepKind::Dev => " [style=dashed, color=blue]",
                DepKind::Build => " [style=bold, color=green]",
                DepKind::Normal => "",
            };

            output.push_str(&format!("    {} -> {}{};\n", from_name, to_name, style));
        }
    }

    output.push_str("}\n");
    output
}

pub fn generate_deps_json(graph_data: &GraphData, args: &DepsArgs) -> String {
    let mut nodes: Vec<serde_json::Value> = Vec::new();
    let mut edges: Vec<serde_json::Value> = Vec::new();

    for idx in graph_data.graph.node_indices() {
        let info = &graph_data.graph[idx];
        nodes.push(serde_json::json!({
            "id": sanitize_name(&info.name),
            "name": info.name,
            "version": info.version,
            "is_workspace_member": info.is_workspace_member,
            "highlighted": args.highlight.contains(&info.name)
        }));
    }

    for edge in graph_data.graph.edge_indices() {
        if let Some((from, to)) = graph_data.graph.edge_endpoints(edge) {
            let kind = graph_data.graph[edge];
            edges.push(serde_json::json!({
                "from": sanitize_name(&graph_data.graph[from].name),
                "to": sanitize_name(&graph_data.graph[to].name),
                "kind": match kind {
                    DepKind::Normal => "normal",
                    DepKind::Dev => "dev",
                    DepKind::Build => "build",
                }
            }));
        }
    }

    serde_json::to_string_pretty(&serde_json::json!({
        "nodes": nodes,
        "edges": edges
    }))
    .unwrap_or_else(|_| "{}".to_string())
}

// ============================================================================
// Function Graph - Output Generators
// ============================================================================

pub fn generate_fn_mermaid(graph_data: &FnGraphData, args: &FnGraphArgs) -> String {
    let mut output = String::new();

    if !args.no_fence {
        output.push_str("```mermaid\n");
    }

    output.push_str(&format!("flowchart {}\n", args.direction));

    // Theme styling
    match args.theme {
        Theme::Dark => {
            output.push_str("    %%{init: {'theme': 'dark'}}%%\n");
        }
        Theme::Light => {
            output.push_str("    %%{init: {'theme': 'default'}}%%\n");
        }
        Theme::Default => {}
    }

    // Edges
    for edge in graph_data.graph.edge_indices() {
        if let Some((from, to)) = graph_data.graph.edge_endpoints(edge) {
            let from_info = &graph_data.graph[from];
            let to_info = &graph_data.graph[to];
            let edge_kind = graph_data.graph[edge];

            let from_label = format_fn_label(from_info, args);
            let to_label = format_fn_label(to_info, args);

            let arrow = match edge_kind {
                CallKind::Direct => "-->",
                CallKind::Method => "-.->",
            };

            output.push_str(&format!("    {} {} {}\n", from_label, arrow, to_label));
        }
    }

    // Highlight styling
    for highlight in &args.highlight {
        let sanitized = sanitize_name(highlight);
        output.push_str(&format!("    style {} fill:#f9f,stroke:#333,stroke-width:4px\n", sanitized));
    }

    if !args.no_fence {
        output.push_str("```\n");
    }

    output
}

pub fn generate_fn_dot(graph_data: &FnGraphData, args: &FnGraphArgs) -> String {
    let mut output = String::new();

    output.push_str("digraph call_graph {\n");
    output.push_str("    rankdir=LR;\n");
    output.push_str("    node [shape=box, style=rounded];\n");

    // Theme
    match args.theme {
        Theme::Dark => {
            output.push_str("    bgcolor=\"#1e1e1e\";\n");
            output.push_str("    node [fontcolor=white, color=white];\n");
            output.push_str("    edge [color=white];\n");
        }
        Theme::Light => {
            output.push_str("    bgcolor=white;\n");
        }
        Theme::Default => {}
    }

    // Node definitions
    let mut defined_nodes: HashSet<String> = HashSet::new();
    for idx in graph_data.graph.node_indices() {
        let info = &graph_data.graph[idx];
        let sanitized = sanitize_name(&info.name);

        if defined_nodes.insert(sanitized.clone()) {
            let label = if args.show_signatures {
                info.signature.as_ref().unwrap_or(&info.name).clone()
            } else {
                info.name.clone()
            };

            let mut node_attrs = vec![format!("label=\"{}\"", label.replace('"', "\\\""))];

            if args.highlight.contains(&info.name) {
                node_attrs.push("fillcolor=\"#ff99ff\"".to_string());
                node_attrs.push("style=\"filled,rounded\"".to_string());
            }

            if info.is_public {
                node_attrs.push("penwidth=2".to_string());
            }

            if info.is_async {
                node_attrs.push("color=blue".to_string());
            }

            output.push_str(&format!("    {} [{}];\n", sanitized, node_attrs.join(", ")));
        }
    }

    // Edges
    for edge in graph_data.graph.edge_indices() {
        if let Some((from, to)) = graph_data.graph.edge_endpoints(edge) {
            let from_name = sanitize_name(&graph_data.graph[from].name);
            let to_name = sanitize_name(&graph_data.graph[to].name);
            let kind = graph_data.graph[edge];

            let style = match kind {
                CallKind::Direct => "",
                CallKind::Method => " [style=dashed]",
            };

            output.push_str(&format!("    {} -> {}{};\n", from_name, to_name, style));
        }
    }

    output.push_str("}\n");
    output
}

pub fn generate_fn_json(graph_data: &FnGraphData, args: &FnGraphArgs) -> String {
    let mut nodes: Vec<serde_json::Value> = Vec::new();
    let mut edges: Vec<serde_json::Value> = Vec::new();

    for idx in graph_data.graph.node_indices() {
        let info = &graph_data.graph[idx];
        let mut node = serde_json::json!({
            "id": sanitize_name(&info.name),
            "name": info.name,
            "qualified_name": info.qualified_name,
            "file": info.file_path,
            "line": info.line,
            "is_public": info.is_public,
            "is_async": info.is_async,
            "highlighted": args.highlight.contains(&info.name)
        });

        if let Some(ref sig) = info.signature {
            node["signature"] = serde_json::json!(sig);
        }

        nodes.push(node);
    }

    for edge in graph_data.graph.edge_indices() {
        if let Some((from, to)) = graph_data.graph.edge_endpoints(edge) {
            let kind = graph_data.graph[edge];
            edges.push(serde_json::json!({
                "from": sanitize_name(&graph_data.graph[from].name),
                "to": sanitize_name(&graph_data.graph[to].name),
                "kind": match kind {
                    CallKind::Direct => "direct",
                    CallKind::Method => "method",
                }
            }));
        }
    }

    serde_json::to_string_pretty(&serde_json::json!({
        "nodes": nodes,
        "edges": edges
    }))
    .unwrap_or_else(|_| "{}".to_string())
}

pub fn format_fn_label(info: &FnNodeInfo, args: &FnGraphArgs) -> String {
    let sanitized = sanitize_name(&info.name);
    if args.show_signatures {
        if let Some(ref sig) = info.signature {
            return sanitize_name(&sig.replace(['(', ')', ',', ' ', '-', '>'], "_"));
        }
    }
    sanitized
}
