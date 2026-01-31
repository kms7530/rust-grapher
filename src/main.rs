mod types;
mod utils {
    pub mod generator;
    pub mod grapher;
    pub mod helper;
}

use cargo_metadata::{MetadataCommand, Package, PackageId};
use petgraph::graph::{DiGraph};
use clap::Parser;

use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::{self, Write};
use std::path::PathBuf;

use types::{Cli, DepsArgs, Commands, OutputFormat, GraphData};

use utils::generator::{generate_deps_mermaid, generate_deps_dot, generate_deps_json};
use utils::grapher::{add_package_to_graph, run_fn_graph, filter_by_focus};

// ============================================================================
// Main
// ============================================================================

fn main() {
    let cli = Cli::parse();

    let result = match &cli.command {
        Commands::Deps(args) => run_deps(args),
        Commands::FnGraph(args) => run_fn_graph(args),
    };

    match result {
        Ok((output, output_path)) => {
            if let Some(ref path) = output_path {
                if let Err(e) = fs::write(path, &output) {
                    eprintln!("Error writing to file: {}", e);
                    std::process::exit(1);
                }
                eprintln!("Graph written to: {}", path.display());
            } else {
                io::stdout().write_all(output.as_bytes()).unwrap();
            }
        }
        Err(e) => {
            eprintln!("Error: {}", e);
            std::process::exit(1);
        }
    }
}

fn run_deps(args: &DepsArgs) -> Result<(String, Option<PathBuf>), Box<dyn std::error::Error>> {
    let metadata = MetadataCommand::new()
        .manifest_path(&args.manifest_path)
        .exec()?;

    let workspace_members: HashSet<_> = metadata.workspace_members.iter().collect();

    // Build package lookup map
    let packages: HashMap<&PackageId, &Package> =
        metadata.packages.iter().map(|p| (&p.id, p)).collect();

    // Get root packages
    let root_packages: Vec<&Package> = if let Some(ref pkg_name) = args.package {
        metadata
            .packages
            .iter()
            .filter(|p| p.name == *pkg_name)
            .collect()
    } else {
        metadata
            .workspace_members
            .iter()
            .filter_map(|id| packages.get(id).copied())
            .collect()
    };

    if root_packages.is_empty() {
        return Err("No packages found".into());
    }

    // Build graph
    let mut graph_data = GraphData {
        graph: DiGraph::new(),
        node_indices: HashMap::new(),
    };

    let resolve = metadata.resolve.as_ref().ok_or("No resolve data")?;

    for root_pkg in &root_packages {
        add_package_to_graph(
            root_pkg,
            &packages,
            &resolve.nodes,
            &workspace_members,
            &mut graph_data,
            args,
            0,
            &mut HashSet::new(),
        );
    }

    // Apply focus filter
    if let Some(ref focus_crate) = args.focus {
        filter_by_focus(&mut graph_data, focus_crate);
    }

    // Generate output
    let output = match args.format {
        OutputFormat::Mermaid => generate_deps_mermaid(&graph_data, args),
        OutputFormat::Dot => generate_deps_dot(&graph_data, args),
        OutputFormat::Json => generate_deps_json(&graph_data, args),
    };

    Ok((output, args.output.clone()))
}

