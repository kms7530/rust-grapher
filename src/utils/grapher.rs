// ============================================================================
// Graph Building
// ============================================================================

use std::{collections::{HashMap, HashSet}, fs, path::PathBuf};

use cargo_metadata::{Package, PackageId, DependencyKind};
use petgraph::graph::{DiGraph, NodeIndex};
use syn::visit::Visit;
use walkdir::WalkDir;

use crate::{types::{self, CallCollector, CallInfo, FunctionCollector, FunctionDef, OutputFormat}, utils};

use types::{DepsArgs, DepKind, NodeInfo, GraphData, FnGraphArgs, FnGraphData, FnNodeInfo, CallKind, };
use utils::helper::{matches_any_pattern, sanitize_name};
use utils::generator::{generate_fn_mermaid, generate_fn_dot, generate_fn_json};

pub fn add_package_to_graph(
    pkg: &Package,
    packages: &HashMap<&PackageId, &Package>,
    nodes: &[cargo_metadata::Node],
    workspace_members: &HashSet<&PackageId>,
    graph_data: &mut GraphData,
    args: &DepsArgs,
    current_depth: usize,
    visited: &mut HashSet<PackageId>,
) {
    // Depth check
    if args.depth > 0 && current_depth > args.depth {
        return;
    }

    // Cycle detection
    if visited.contains(&pkg.id) {
        return;
    }

    // Exclusion check (supports wildcards: *tauri*, serde-*)
    if matches_any_pattern(&pkg.name.to_string(), &args.exclude) {
        return;
    }

    // Include filter (supports wildcards)
    if !args.include.is_empty() && !matches_any_pattern(&pkg.name.to_string(), &args.include) {
        // Still process if this is depth 0 (root package)
        if current_depth > 0 {
            return;
        }
    }

    // Workspace-only filter
    if args.workspace_only && !workspace_members.contains(&pkg.id) && current_depth > 0 {
        return;
    }

    visited.insert(pkg.id.clone());

    // Add node
    let is_workspace = workspace_members.contains(&pkg.id);
    let node_info = NodeInfo {
        name: pkg.name.to_string(),
        version: pkg.version.to_string(),
        kind: DepKind::Normal,
        is_workspace_member: is_workspace,
    };

    let node_idx = *graph_data
        .node_indices
        .entry(pkg.id.clone())
        .or_insert_with(|| graph_data.graph.add_node(node_info));

    // No transitive check
    if args.no_transitive && current_depth >= 1 {
        return;
    }

    // Find dependencies
    if let Some(node) = nodes.iter().find(|n| n.id == pkg.id) {
        for dep in &node.deps {
            let dep_kind = dep
                .dep_kinds
                .iter()
                .map(|dk| &dk.kind)
                .next()
                .unwrap_or(&DependencyKind::Normal);

            let kind = match dep_kind {
                DependencyKind::Development => DepKind::Dev,
                DependencyKind::Build => DepKind::Build,
                _ => DepKind::Normal,
            };

            // Filter by dependency kind
            if args.no_dev && kind == DepKind::Dev {
                continue;
            }
            if args.no_build && kind == DepKind::Build {
                continue;
            }

            // Exclusion check for dependency (supports wildcards)
            if let Some(dep_pkg) = packages.get(&dep.pkg) {
                if matches_any_pattern(&dep_pkg.name.to_string(), &args.exclude) {
                    continue;
                }

                let dep_is_workspace = workspace_members.contains(&dep.pkg);

                // Workspace-only filter for dependency
                if args.workspace_only && !dep_is_workspace {
                    continue;
                }

                // Dedup check
                let dep_node_idx = if args.dedup {
                    // Check if we already have this crate (by name)
                    if let Some(existing) = graph_data.node_indices.get(&dep.pkg) {
                        *existing
                    } else {
                        let dep_info = NodeInfo {
                            name: dep_pkg.name.to_string(),
                            version: dep_pkg.version.to_string(),
                            kind,
                            is_workspace_member: dep_is_workspace,
                        };
                        let idx = graph_data.graph.add_node(dep_info);
                        graph_data.node_indices.insert(dep.pkg.clone(), idx);
                        idx
                    }
                } else {
                    let dep_info = NodeInfo {
                        name: dep_pkg.name.to_string(),
                        version: dep_pkg.version.to_string(),
                        kind,
                        is_workspace_member: dep_is_workspace,
                    };
                    *graph_data
                        .node_indices
                        .entry(dep.pkg.clone())
                        .or_insert_with(|| graph_data.graph.add_node(dep_info))
                };

                // Add edge if not exists
                if !graph_data.graph.contains_edge(node_idx, dep_node_idx) {
                    graph_data.graph.add_edge(node_idx, dep_node_idx, kind);
                }

                // Recurse
                add_package_to_graph(
                    dep_pkg,
                    packages,
                    nodes,
                    workspace_members,
                    graph_data,
                    args,
                    current_depth + 1,
                    visited,
                );
            }
        }
    }
}

pub fn filter_by_focus(graph_data: &mut GraphData, focus_crate: &str) {
    let focus_name = sanitize_name(focus_crate);

    // Find the focus node
    let focus_nodes: Vec<_> = graph_data
        .graph
        .node_indices()
        .filter(|&idx| sanitize_name(&graph_data.graph[idx].name) == focus_name)
        .collect();

    if focus_nodes.is_empty() {
        return;
    }

    // Collect all connected nodes (both directions)
    let mut connected: HashSet<NodeIndex> = HashSet::new();
    for &focus_idx in &focus_nodes {
        connected.insert(focus_idx);
        collect_connected(&graph_data.graph, focus_idx, &mut connected);
    }

    // Remove unconnected nodes
    let to_remove: Vec<_> = graph_data
        .graph
        .node_indices()
        .filter(|idx| !connected.contains(idx))
        .collect();

    for idx in to_remove.into_iter().rev() {
        graph_data.graph.remove_node(idx);
    }
}

fn collect_connected(graph: &DiGraph<NodeInfo, DepKind>, start: NodeIndex, connected: &mut HashSet<NodeIndex>) {
    // Outgoing edges
    for neighbor in graph.neighbors(start) {
        if connected.insert(neighbor) {
            collect_connected(graph, neighbor, connected);
        }
    }
    // Incoming edges
    for neighbor in graph.neighbors_directed(start, petgraph::Direction::Incoming) {
        if connected.insert(neighbor) {
            collect_connected(graph, neighbor, connected);
        }
    }
}

// ============================================================================
// Function Graph - Visitor Implementation
// ============================================================================
impl FunctionCollector {
    fn new() -> Self {
        FunctionCollector {
            module_path: Vec::new(),
            functions: Vec::new(),
            current_impl_type: None,
        }
    }

    fn qualified_name(&self, name: &str) -> String {
        let mut parts = self.module_path.clone();
        if let Some(ref impl_type) = self.current_impl_type {
            parts.push(impl_type.clone());
        }
        parts.push(name.to_string());
        parts.join("::")
    }

    fn format_signature(sig: &syn::Signature) -> String {
        let inputs: Vec<String> = sig.inputs.iter().map(|arg| {
            match arg {
                syn::FnArg::Receiver(r) => {
                    if r.reference.is_some() {
                        if r.mutability.is_some() { "&mut self".to_string() }
                        else { "&self".to_string() }
                    } else {
                        "self".to_string()
                    }
                }
                syn::FnArg::Typed(pat) => {
                    format!("{}", quote::quote!(#pat))
                }
            }
        }).collect();

        let output = match &sig.output {
            syn::ReturnType::Default => String::new(),
            syn::ReturnType::Type(_, ty) => format!(" -> {}", quote::quote!(#ty)),
        };

        format!("fn {}({}){}", sig.ident, inputs.join(", "), output)
    }
}

impl<'ast> Visit<'ast> for FunctionCollector {
    fn visit_item_fn(&mut self, node: &'ast syn::ItemFn) {
        let is_public = matches!(node.vis, syn::Visibility::Public(_));
        let name = node.sig.ident.to_string();
        let qualified = self.qualified_name(&name);

        self.functions.push(FunctionDef {
            name,
            qualified_name: qualified,
            is_public,
            line: 0, // Line info requires span-locations feature
            signature: Self::format_signature(&node.sig),
            is_async: node.sig.asyncness.is_some(),
        });

        syn::visit::visit_item_fn(self, node);
    }

    fn visit_item_impl(&mut self, node: &'ast syn::ItemImpl) {
        // Extract impl type name
        let type_name = if let syn::Type::Path(type_path) = &*node.self_ty {
            type_path.path.segments.last()
                .map(|seg| seg.ident.to_string())
        } else {
            None
        };

        let old_impl = self.current_impl_type.take();
        self.current_impl_type = type_name;

        syn::visit::visit_item_impl(self, node);

        self.current_impl_type = old_impl;
    }

    fn visit_impl_item_fn(&mut self, node: &'ast syn::ImplItemFn) {
        let is_public = matches!(node.vis, syn::Visibility::Public(_));
        let name = node.sig.ident.to_string();
        let qualified = self.qualified_name(&name);

        self.functions.push(FunctionDef {
            name,
            qualified_name: qualified,
            is_public,
            line: 0, // Line info requires span-locations feature
            signature: FunctionCollector::format_signature(&node.sig),
            is_async: node.sig.asyncness.is_some(),
        });

        syn::visit::visit_impl_item_fn(self, node);
    }

    fn visit_item_mod(&mut self, node: &'ast syn::ItemMod) {
        self.module_path.push(node.ident.to_string());
        syn::visit::visit_item_mod(self, node);
        self.module_path.pop();
    }
}

impl CallCollector {
    fn new(current_function: String) -> Self {
        CallCollector {
            current_function,
            calls: Vec::new(),
        }
    }
}

impl<'ast> Visit<'ast> for CallCollector {
    fn visit_expr_call(&mut self, node: &'ast syn::ExprCall) {
        // Extract callee name from the function expression
        let callee = extract_call_name(&node.func);
        if let Some(name) = callee {
            self.calls.push(CallInfo {
                caller: self.current_function.clone(),
                callee: name,
                kind: CallKind::Direct,
            });
        }
        syn::visit::visit_expr_call(self, node);
    }

    fn visit_expr_method_call(&mut self, node: &'ast syn::ExprMethodCall) {
        let method_name = node.method.to_string();
        self.calls.push(CallInfo {
            caller: self.current_function.clone(),
            callee: method_name,
            kind: CallKind::Method,
        });
        syn::visit::visit_expr_method_call(self, node);
    }
}

fn extract_call_name(expr: &syn::Expr) -> Option<String> {
    match expr {
        syn::Expr::Path(path) => {
            Some(path.path.segments.iter()
                .map(|seg| seg.ident.to_string())
                .collect::<Vec<_>>()
                .join("::"))
        }
        _ => None,
    }
}

// ============================================================================
// Function Graph - Main Logic
// ============================================================================

pub fn run_fn_graph(args: &FnGraphArgs) -> Result<(String, Option<PathBuf>), Box<dyn std::error::Error>> {
    let source_dir = &args.source_dir;

    if !source_dir.exists() {
        return Err(format!("Source directory not found: {}", source_dir.display()).into());
    }

    // Collect all Rust files
    let rust_files: Vec<_> = WalkDir::new(source_dir)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().map_or(false, |ext| ext == "rs"))
        .collect();

    let mut all_functions: Vec<(FunctionDef, String)> = Vec::new();
    let mut all_calls: Vec<CallInfo> = Vec::new();

    // Parse each file
    for entry in rust_files {
        let file_path = entry.path();
        let content = match fs::read_to_string(file_path) {
            Ok(c) => c,
            Err(_) => continue,
        };

        let syntax = match syn::parse_file(&content) {
            Ok(s) => s,
            Err(_) => continue,
        };

        let relative_path = file_path.strip_prefix(source_dir)
            .unwrap_or(file_path)
            .to_string_lossy()
            .to_string();

        // Collect function definitions
        let mut collector = FunctionCollector::new();
        collector.visit_file(&syntax);

        for func in collector.functions {
            all_functions.push((func, relative_path.clone()));
        }
    }

    // Collect function calls by re-parsing with call collector
    for entry in WalkDir::new(source_dir)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().map_or(false, |ext| ext == "rs"))
    {
        let file_path = entry.path();
        let content = match fs::read_to_string(file_path) {
            Ok(c) => c,
            Err(_) => continue,
        };

        let syntax = match syn::parse_file(&content) {
            Ok(s) => s,
            Err(_) => continue,
        };

        // For each function, collect calls
        collect_calls_from_file(&syntax, &mut all_calls, &all_functions);
    }

    // Build graph
    let mut graph_data = FnGraphData {
        graph: DiGraph::new(),
        node_indices: HashMap::new(),
    };

    // Create function name -> qualified_name lookup
    let fn_lookup: HashMap<String, String> = all_functions.iter()
        .map(|(f, _)| (f.name.clone(), f.qualified_name.clone()))
        .collect();

    // Add nodes
    for (func, file_path) in &all_functions {
        // Apply filters
        if args.public_only && !func.is_public {
            continue;
        }
        if matches_any_pattern(&func.name, &args.exclude) {
            continue;
        }
        if matches_any_pattern(&func.qualified_name, &args.exclude) {
            continue;
        }

        let node_info = FnNodeInfo {
            name: func.name.clone(),
            qualified_name: func.qualified_name.clone(),
            file_path: file_path.clone(),
            line: func.line,
            is_public: func.is_public,
            signature: if args.show_signatures { Some(func.signature.clone()) } else { None },
            is_async: func.is_async,
        };

        let idx = graph_data.graph.add_node(node_info);
        graph_data.node_indices.insert(func.qualified_name.clone(), idx);
    }

    // Add edges
    for call in &all_calls {
        // Try to resolve callee to a known function
        let callee_qualified = fn_lookup.get(&call.callee)
            .cloned()
            .unwrap_or_else(|| call.callee.clone());

        if let (Some(&from_idx), Some(&to_idx)) = (
            graph_data.node_indices.get(&call.caller),
            graph_data.node_indices.get(&callee_qualified),
        ) {
            // Avoid self-loops and duplicate edges
            if from_idx != to_idx && !graph_data.graph.contains_edge(from_idx, to_idx) {
                graph_data.graph.add_edge(from_idx, to_idx, call.kind);
            }
        }
    }

    // Apply focus filter
    if let Some(ref focus_fn) = args.focus {
        filter_fn_by_focus(&mut graph_data, focus_fn, args.depth);
    }

    // Generate output
    let output = match args.format {
        OutputFormat::Mermaid => generate_fn_mermaid(&graph_data, args),
        OutputFormat::Dot => generate_fn_dot(&graph_data, args),
        OutputFormat::Json => generate_fn_json(&graph_data, args),
    };

    Ok((output, args.output.clone()))
}

fn collect_calls_from_file(
    file: &syn::File,
    all_calls: &mut Vec<CallInfo>,
    all_functions: &[(FunctionDef, String)],
) {
    // Create a set of known function qualified names
    let known_fns: HashSet<String> = all_functions.iter()
        .map(|(f, _)| f.qualified_name.clone())
        .collect();

    // Visit each function and collect calls
    for item in &file.items {
        collect_calls_from_item(item, all_calls, &known_fns, &[]);
    }
}

fn collect_calls_from_item(
    item: &syn::Item,
    all_calls: &mut Vec<CallInfo>,
    known_fns: &HashSet<String>,
    module_path: &[String],
) {
    match item {
        syn::Item::Fn(item_fn) => {
            let mut path = module_path.to_vec();
            path.push(item_fn.sig.ident.to_string());
            let qualified = path.join("::");

            let mut collector = CallCollector::new(qualified);
            collector.visit_item_fn(item_fn);
            all_calls.extend(collector.calls);
        }
        syn::Item::Impl(item_impl) => {
            let type_name = if let syn::Type::Path(type_path) = &*item_impl.self_ty {
                type_path.path.segments.last()
                    .map(|seg| seg.ident.to_string())
            } else {
                None
            };

            for impl_item in &item_impl.items {
                if let syn::ImplItem::Fn(method) = impl_item {
                    let mut path = module_path.to_vec();
                    if let Some(ref tn) = type_name {
                        path.push(tn.clone());
                    }
                    path.push(method.sig.ident.to_string());
                    let qualified = path.join("::");

                    let mut collector = CallCollector::new(qualified);
                    collector.visit_impl_item_fn(method);
                    all_calls.extend(collector.calls);
                }
            }
        }
        syn::Item::Mod(item_mod) => {
            if let Some((_, items)) = &item_mod.content {
                let mut path = module_path.to_vec();
                path.push(item_mod.ident.to_string());
                for sub_item in items {
                    collect_calls_from_item(sub_item, all_calls, known_fns, &path);
                }
            }
        }
        _ => {}
    }
}

fn filter_fn_by_focus(graph_data: &mut FnGraphData, focus_fn: &str, max_depth: usize) {
    // Find the focus node(s)
    let focus_nodes: Vec<NodeIndex> = graph_data
        .graph
        .node_indices()
        .filter(|&idx| {
            let info = &graph_data.graph[idx];
            info.name == focus_fn || info.qualified_name == focus_fn
                || info.qualified_name.ends_with(&format!("::{}", focus_fn))
        })
        .collect();

    if focus_nodes.is_empty() {
        return;
    }

    // Collect connected nodes with depth limit
    let mut connected: HashSet<NodeIndex> = HashSet::new();
    for &focus_idx in &focus_nodes {
        connected.insert(focus_idx);
        collect_fn_connected(&graph_data.graph, focus_idx, &mut connected, 0, max_depth);
    }

    // Remove unconnected nodes
    let to_remove: Vec<_> = graph_data
        .graph
        .node_indices()
        .filter(|idx| !connected.contains(idx))
        .collect();

    for idx in to_remove.into_iter().rev() {
        graph_data.graph.remove_node(idx);
    }
}

fn collect_fn_connected(
    graph: &DiGraph<FnNodeInfo, CallKind>,
    start: NodeIndex,
    connected: &mut HashSet<NodeIndex>,
    current_depth: usize,
    max_depth: usize,
) {
    if max_depth > 0 && current_depth >= max_depth {
        return;
    }

    // Outgoing edges (callees)
    for neighbor in graph.neighbors(start) {
        if connected.insert(neighbor) {
            collect_fn_connected(graph, neighbor, connected, current_depth + 1, max_depth);
        }
    }
    // Incoming edges (callers)
    for neighbor in graph.neighbors_directed(start, petgraph::Direction::Incoming) {
        if connected.insert(neighbor) {
            collect_fn_connected(graph, neighbor, connected, current_depth + 1, max_depth);
        }
    }
}
