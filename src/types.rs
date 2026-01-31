use std::path::PathBuf;
use cargo_metadata::PackageId;
use clap::{Args, Parser, Subcommand, ValueEnum};
use petgraph::graph::{DiGraph, NodeIndex};
use std::collections::{HashMap};

// ============================================================================
// CLI Arguments
// ============================================================================

#[derive(Parser)]
#[command(name = "rust-grapher")]
#[command(version = "0.1.0")]
#[command(author = "kms7530 <kwak@minseok.me>")]
#[command(about = "Generate dependency and function call graphs for Rust projects")]
#[command(long_about = r#"
rust-grapher analyzes Rust projects and generates graphs in multiple formats.

EXAMPLES:
    # Generate dependency graph (module mode)
    rust-grapher deps
    rust-grapher deps --depth 2 -o deps.md
    rust-grapher deps --workspace-only

    # Generate function call graph (function mode)
    rust-grapher fn-graph
    rust-grapher fn-graph --focus main --depth 3
    rust-grapher fn-graph -f dot | dot -Tpng -o call-graph.png
"#)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Analyze Cargo dependency graph (module mode)
    Deps(DepsArgs),
    /// Analyze function call graph (function mode)
    FnGraph(FnGraphArgs),
}

#[derive(Args)]
pub struct DepsArgs {
    // === Input Options ===
    /// Path to Cargo.toml
    #[arg(long, short = 'm', default_value = "Cargo.toml")]
    pub manifest_path: PathBuf,

    /// Focus on specific package (workspace member)
    #[arg(long, short = 'p')]
    pub package: Option<String>,

    // === Output Options ===
    /// Output file path (stdout if not specified)
    #[arg(long, short = 'o')]
    pub output: Option<PathBuf>,

    /// Output format
    #[arg(long, short = 'f', value_enum, default_value = "mermaid")]
    pub format: OutputFormat,

    /// Omit code fence markers (```mermaid)
    #[arg(long)]
    pub(crate) no_fence: bool,

    /// Graph direction: LR (left-right) or TB (top-bottom)
    #[arg(long, short = 'd', default_value = "LR")]
    pub direction: String,

    // === Filtering Options ===
    /// Maximum dependency depth (0 = unlimited)
    #[arg(long, default_value = "0")]
    pub depth: usize,

    /// Exclude dev-dependencies
    #[arg(long)]
    pub no_dev: bool,

    /// Exclude build-dependencies
    #[arg(long)]
    pub no_build: bool,

    /// Exclude crates matching pattern (supports * wildcard, can be used multiple times)
    #[arg(long, short = 'e')]
    pub exclude: Vec<String>,

    /// Include only crates matching pattern (supports * wildcard, can be used multiple times)
    #[arg(long, short = 'i')]
    pub include: Vec<String>,

    /// Show only crates connected to this crate
    #[arg(long)]
    pub focus: Option<String>,

    /// Show only workspace members
    #[arg(long)]
    pub workspace_only: bool,

    /// Show only direct dependencies (no transitive)
    #[arg(long)]
    pub no_transitive: bool,

    // === Display Options ===
    /// Show version numbers with crate names
    #[arg(long, short = 'v')]
    pub show_versions: bool,

    /// Group dependencies by kind (dev/build/normal) using subgraphs
    #[arg(long)]
    pub group_by_kind: bool,

    /// Deduplicate: show each crate only once
    #[arg(long)]
    pub dedup: bool,

    // === Style Options ===
    /// Color theme
    #[arg(long, value_enum, default_value = "default")]
    pub theme: Theme,

    /// Highlight specific crates (can be used multiple times)
    #[arg(long, short = 'H')]
    pub highlight: Vec<String>,
}

#[derive(Args)]
pub struct FnGraphArgs {
    /// Source directory to analyze
    #[arg(long, short = 's', default_value = "src")]
    pub source_dir: PathBuf,

    /// Output file path (stdout if not specified)
    #[arg(long, short = 'o')]
    pub output: Option<PathBuf>,

    /// Output format
    #[arg(long, short = 'f', value_enum, default_value = "mermaid")]
    pub format: OutputFormat,

    /// Omit code fence markers (```mermaid)
    #[arg(long)]
    pub no_fence: bool,

    /// Graph direction: LR (left-right) or TB (top-bottom)
    #[arg(long, short = 'd', default_value = "LR")]
    pub direction: String,

    /// Focus on specific function (show only connected functions)
    #[arg(long)]
    pub focus: Option<String>,

    /// Maximum call depth (0 = unlimited)
    #[arg(long, default_value = "0")]
    pub depth: usize,

    /// Exclude functions matching pattern (supports * wildcard)
    #[arg(long, short = 'e')]
    pub exclude: Vec<String>,

    /// Include only public functions
    #[arg(long)]
    pub public_only: bool,

    /// Show function signatures
    #[arg(long)]
    pub show_signatures: bool,

    /// Color theme
    #[arg(long, value_enum, default_value = "default")]
    pub theme: Theme,

    /// Highlight specific functions (can be used multiple times)
    #[arg(long, short = 'H')]
    pub highlight: Vec<String>,
}

#[derive(Clone, ValueEnum)]
pub enum OutputFormat {
    Mermaid,
    Dot,
    Json,
}

#[derive(Clone, ValueEnum)]
pub enum Theme {

    Default,
    Light,
    Dark,
}

// ============================================================================
// Data Structures - Deps
// ============================================================================

#[derive(Clone)]
pub struct NodeInfo {
    pub name: String,
    pub version: String,
    #[allow(dead_code)]
    pub kind: DepKind,
    pub is_workspace_member: bool,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub enum DepKind {
    Normal,
    Dev,
    Build,
}

pub struct GraphData {
    pub graph: DiGraph<NodeInfo, DepKind>,
    pub node_indices: HashMap<PackageId, NodeIndex>,
}

// ============================================================================
// Data Structures - Function Graph
// ============================================================================

#[derive(Clone)]
pub struct FnNodeInfo {
    pub name: String,
    pub qualified_name: String,
    pub file_path: String,
    pub line: usize,
    pub is_public: bool,
    pub signature: Option<String>,
    pub is_async: bool,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub enum CallKind {
    Direct,
    Method,
}

pub struct FnGraphData {
    pub graph: DiGraph<FnNodeInfo, CallKind>,
    pub node_indices: HashMap<String, NodeIndex>,
}

#[derive(Clone)]
pub struct FunctionDef {
    pub name: String,
    pub qualified_name: String,
    pub is_public: bool,
    pub line: usize,
    pub signature: String,
    pub is_async: bool,
}

pub struct CallInfo {
    pub caller: String,
    pub callee: String,
    pub kind: CallKind,
}

pub struct FunctionCollector {
    pub module_path: Vec<String>,
    pub functions: Vec<FunctionDef>,
    pub current_impl_type: Option<String>,
}

pub struct CallCollector {
    pub current_function: String,
    pub calls: Vec<CallInfo>,
}