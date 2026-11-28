//! Bounded Rust dependency indexing and deterministic two-hop context slicing.
use air_weights::{
    ArchiveError, ArchiveReader, DependencyEdge, DependencyEdgeKind, DependencyGraph,
};
use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    error::Error,
    fmt,
};
use tree_sitter::{Node, Parser};

pub const MAX_SOURCE_FILES: usize = 256;
pub const MAX_SOURCE_FILE_BYTES: usize = 256 * 1024;
pub const MAX_TOTAL_SOURCE_BYTES: usize = 2 * 1024 * 1024;
pub const MAX_SLICE_NODES: usize = 256;
pub const MAX_SLICE_EDGES: usize = 1024;
pub const DEFAULT_PROMPT_CONTEXT_BYTES: usize = 12 * 1024;

#[derive(Debug)]
pub enum DependencyError {
    InvalidInput(&'static str),
    Limit(&'static str),
    Parse(&'static str),
    Archive(ArchiveError),
}
impl fmt::Display for DependencyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidInput(s) => write!(f, "invalid dependency input: {s}"),
            Self::Limit(s) => write!(f, "dependency limit exceeded: {s}"),
            Self::Parse(s) => write!(f, "Rust dependency parse failed: {s}"),
            Self::Archive(e) => write!(f, "AIRW dependency graph error: {e}"),
        }
    }
}
impl Error for DependencyError {}
impl From<ArchiveError> for DependencyError {
    fn from(value: ArchiveError) -> Self { Self::Archive(value) }
}

#[derive(Debug, Clone, Copy)]
pub struct RustSource<'a> {
    pub path: &'a str,
    pub text: &'a str,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContextSlice {
    pub target: String,
    /// Sorted identifiers at graph distance one or two from target.
    pub nodes: Vec<String>,
    /// Directed edges retained within the two-hop neighborhood.
    pub edges: Vec<DependencyEdge>,
}

impl ContextSlice {
    /// Stable, bounded, plain-text context for a caller to add to an SLM prompt.
    /// AIR does not depend on air-slm; callers remain responsible for delimiting
    /// this source-derived material as untrusted context.
    pub fn to_prompt_context(&self, max_bytes: usize) -> Result<String, DependencyError> {
        if max_bytes == 0 || max_bytes > 64 * 1024 {
            return Err(DependencyError::Limit("prompt context bytes"));
        }
        let mut out = String::new();
        let header = format!("Relevant Rust dependency slice for {}:\n", self.target);
        if header.len() > max_bytes {
            return Err(DependencyError::Limit("prompt context bytes"));
        }
        out.push_str(&header);
        for node in &self.nodes {
            let line = format!("symbol {node}\n");
            if out.len() + line.len() > max_bytes { break; }
            out.push_str(&line);
        }
        for edge in &self.edges {
            let kind = match edge.kind {
                DependencyEdgeKind::Calls => "calls",
                DependencyEdgeKind::StructTraitImpl => "implements_trait",
                DependencyEdgeKind::FunctionUsesType => "uses_type",
            };
            let line = format!("{kind}: {} -> {}\n", edge.from, edge.to);
            if out.len() + line.len() > max_bytes { break; }
            out.push_str(&line);
        }
        Ok(out)
    }
}

/// Indexes Rust declarations and references with tree-sitter. It records only
/// references whose syntax can be resolved unambiguously; it does not claim
/// whole-workspace name resolution or macro expansion.
pub struct RustDependencyIndexer;

impl RustDependencyIndexer {
    pub fn index_sources(files: &[RustSource<'_>]) -> Result<DependencyGraph, DependencyError> {
        if files.is_empty() || files.len() > MAX_SOURCE_FILES {
            return Err(DependencyError::Limit("source file count"));
        }
        let mut total = 0usize;
        let mut paths = BTreeSet::new();
        let mut functions = Vec::new();
        let mut types: BTreeMap<String, Vec<String>> = BTreeMap::new();
        let mut impl_edges = Vec::new();

        for file in files {
            if file.path.is_empty() || file.path.len() > 256
                || file.path.chars().any(char::is_control)
                || !paths.insert(file.path)
            {
                return Err(DependencyError::InvalidInput("source path"));
            }
            if file.text.len() > MAX_SOURCE_FILE_BYTES {
                return Err(DependencyError::Limit("source file bytes"));
            }
            total = total.checked_add(file.text.len())
                .ok_or(DependencyError::Limit("total source bytes"))?;
            if total > MAX_TOTAL_SOURCE_BYTES {
                return Err(DependencyError::Limit("total source bytes"));
            }

            let mut parser = Parser::new();
            parser.set_language(&tree_sitter_rust::LANGUAGE.into())
                .map_err(|_| DependencyError::Parse("Rust grammar unavailable"))?;
            let tree = parser.parse(file.text, None)
                .ok_or(DependencyError::Parse("parser returned no tree"))?;
            let root = tree.root_node();
            if root.has_error() {
                return Err(DependencyError::Parse("syntax error"));
            }
            let nodes = preorder(root);
            let mut name_counts = BTreeMap::<String, usize>::new();
            for node in &nodes {
                if node.kind() == "function_item" {
                    if let Some(name) = node.child_by_field_name("name") {
                        *name_counts.entry(node_text(name, file.text).to_owned()).or_default() += 1;
                    }
                } else if matches!(node.kind(), "struct_item" | "enum_item" | "type_item" | "trait_item") {
                    if let Some(name) = node.child_by_field_name("name") {
                        let text = node_text(name, file.text);
                        types.entry(text.to_owned()).or_default().push(format!("{}::{}", file.path, text));
                    }
                } else if node.kind() == "impl_item" {
                    if let (Some(trait_node), Some(type_node)) = (
                        node.child_by_field_name("trait"),
                        node.child_by_field_name("type"),
                    ) {
                        let trait_text = last_type_name(node_text(trait_node, file.text));
                        let type_text = last_type_name(node_text(type_node, file.text));
                        impl_edges.push((file.path.to_owned(), type_text, trait_text));
                    }
                }
            }

            for node in &nodes {
                if node.kind() != "function_item" { continue; }
                let Some(name_node) = node.child_by_field_name("name") else { continue; };
                let name = node_text(name_node, file.text).to_owned();
                let start = node.start_byte();
                let id = if name_counts.get(&name).copied().unwrap_or(0) > 1 {
                    format!("{}::{}#{}", file.path, name, start)
                } else {
                    format!("{}::{}", file.path, name)
                };
                let mut relevant = Vec::new();
                collect_function_contents(*node, true, &mut relevant);
                let mut calls = BTreeSet::new();
                let mut used_types = BTreeSet::new();
                for descendant in relevant {
                    if descendant.kind() == "call_expression" {
                        if let Some(function) = descendant.child_by_field_name("function") {
                            let raw = node_text(function, file.text);
                            if let Some(name) = call_name(raw) {
                                calls.insert(name.to_owned());
                            }
                        }
                    } else if descendant.kind() == "type_identifier" {
                        let value = node_text(descendant, file.text);
                        if !value.is_empty() { used_types.insert(value.to_owned()); }
                    } else if descendant.kind() == "scoped_type_identifier" {
                        let value = node_text(descendant, file.text);
                        if !value.is_empty() { used_types.insert(last_type_name(value).to_owned()); }
                    }
                }
                functions.push(RawFunction { id, name, file: file.path.to_owned(), calls, used_types });
            }
        }

        let mut by_name = BTreeMap::<String, Vec<String>>::new();
        for function in &functions {
            by_name.entry(function.name.clone()).or_default().push(function.id.clone());
        }
        for ids in by_name.values_mut() { ids.sort(); ids.dedup(); }

        let mut edges = Vec::new();
        for function in &functions {
            for call in &function.calls {
                if let Some(target) = resolve_unique(call, &by_name) {
                    edges.push(DependencyEdge::new(
                        function.id.clone(), target, DependencyEdgeKind::Calls,
                    ).map_err(DependencyError::InvalidInput)?);
                }
            }
            for used in &function.used_types {
                let target = match types.get(used) {
                    Some(candidates) if candidates.len() == 1 => candidates[0].clone(),
                    _ => used.clone(),
                };
                edges.push(DependencyEdge::new(
                    function.id.clone(), target, DependencyEdgeKind::FunctionUsesType,
                ).map_err(DependencyError::InvalidInput)?);
            }
        }
        for (file, struct_name, trait_name) in impl_edges {
            let from = match types.get(&struct_name) {
                Some(candidates) if candidates.len() == 1 => candidates[0].clone(),
                _ => format!("{file}::{struct_name}"),
            };
            let to = match types.get(&trait_name) {
                Some(candidates) if candidates.len() == 1 => candidates[0].clone(),
                _ => trait_name,
            };
            edges.push(DependencyEdge::new(
                from, to, DependencyEdgeKind::StructTraitImpl,
            ).map_err(DependencyError::InvalidInput)?);
        }
        DependencyGraph::new(edges).map_err(DependencyError::InvalidInput)
    }
}

#[derive(Debug)]
struct RawFunction {
    id: String,
    name: String,
    #[allow(dead_code)]
    file: String,
    calls: BTreeSet<String>,
    used_types: BTreeSet<String>,
}

/// Extract the undirected two-hop neighborhood around a symbol while preserving
/// each retained dependency edge's original direction and type.
pub fn two_hop_slice(
    graph: &DependencyGraph,
    target: &str,
    max_nodes: usize,
    max_edges: usize,
) -> Result<ContextSlice, DependencyError> {
    if target.is_empty() || target.len() > 512 || target.chars().any(char::is_control) {
        return Err(DependencyError::InvalidInput("slice target"));
    }
    if max_nodes == 0 || max_nodes > MAX_SLICE_NODES || max_edges == 0 || max_edges > MAX_SLICE_EDGES {
        return Err(DependencyError::Limit("slice size"));
    }
    graph.validate().map_err(DependencyError::InvalidInput)?;
    let mut distance = BTreeMap::<String, usize>::new();
    let mut queue = VecDeque::new();
    distance.insert(target.to_owned(), 0);
    queue.push_back(target.to_owned());
    while let Some(node) = queue.pop_front() {
        let hops = distance[&node];
        if hops == 2 { continue; }
        let mut neighbors = BTreeSet::new();
        for edge in graph.edges() {
            if edge.from == node { neighbors.insert(edge.to.clone()); }
            if edge.to == node { neighbors.insert(edge.from.clone()); }
        }
        for neighbor in neighbors {
            if !distance.contains_key(&neighbor) {
                if distance.len() >= max_nodes + 1 {
                    return Err(DependencyError::Limit("slice node count"));
                }
                distance.insert(neighbor.clone(), hops + 1);
                queue.push_back(neighbor);
            }
        }
    }
    let nodes: Vec<String> = distance.keys().filter(|n| n.as_str() != target).cloned().collect();
    let mut edges = Vec::new();
    for edge in graph.edges() {
        if distance.contains_key(&edge.from) && distance.contains_key(&edge.to) {
            if edges.len() == max_edges {
                return Err(DependencyError::Limit("slice edge count"));
            }
            edges.push(edge.clone());
        }
    }
    Ok(ContextSlice { target: target.to_owned(), nodes, edges })
}

/// Read an optional graph from AIRW. Archives predating the graph section return None.
pub fn graph_from_archive(reader: &ArchiveReader) -> Result<Option<DependencyGraph>, DependencyError> {
    air_weights::dependency::read_from_archive(reader).map_err(DependencyError::from)
}

fn preorder<'tree>(root: Node<'tree>) -> Vec<Node<'tree>> {
    let mut output = Vec::new();
    let mut stack = vec![root];
    while let Some(node) = stack.pop() {
        output.push(node);
        for i in (0..node.child_count()).rev() {
            if let Some(child) = node.child(i) { stack.push(child); }
        }
    }
    output
}

fn collect_function_contents<'tree>(root: Node<'tree>, is_root: bool, output: &mut Vec<Node<'tree>>) {
    if !is_root && root.kind() == "function_item" { return; }
    output.push(root);
    for i in 0..root.child_count() {
        if let Some(child) = root.child(i) {
            collect_function_contents(child, false, output);
        }
    }
}

fn node_text<'a>(node: Node<'_>, source: &'a str) -> &'a str {
    node.utf8_text(source.as_bytes()).unwrap_or("")
}

fn call_name(raw: &str) -> Option<&str> {
    let raw = raw.trim();
    if raw.is_empty() { return None; }
    let without_generics = raw.split('<').next().unwrap_or(raw).trim();
    let last = without_generics.rsplit("::").next().unwrap_or(without_generics);
    let last = last.rsplit('.').next().unwrap_or(last);
    let last = last.rsplit(' ').next().unwrap_or(last);
    if last.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') { Some(last) } else { None }
}

fn last_type_name(raw: &str) -> String {
    let raw = raw.split('<').next().unwrap_or(raw);
    raw.rsplit("::").next().unwrap_or(raw).trim().to_owned()
}

fn resolve_unique(name: &str, by_name: &BTreeMap<String, Vec<String>>) -> Option<String> {
    match by_name.get(name) {
        Some(ids) if ids.len() == 1 => Some(ids[0].clone()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn indexes_cross_file_calls_types_and_trait_impls() {
        let files = [
            RustSource { path: "src/main.rs", text: "use crate::view::View; pub fn run(view: View) { helper(); }" },
            RustSource { path: "src/worker.rs", text: "use crate::types::Widget; pub fn helper() -> Widget { Widget {} }" },
            RustSource { path: "src/types.rs", text: "pub struct Widget; pub trait Render {} impl Render for Widget {}" },
            RustSource { path: "src/view.rs", text: "pub struct View;" },
        ];
        let graph = RustDependencyIndexer::index_sources(&files).unwrap();
        assert!(graph.edges().iter().any(|e|
            e.from == "src/main.rs::run" && e.to == "src/worker.rs::helper" && e.kind == DependencyEdgeKind::Calls
        ));
        assert!(graph.edges().iter().any(|e|
            e.from == "src/worker.rs::helper" && e.to == "src/types.rs::Widget" && e.kind == DependencyEdgeKind::FunctionUsesType
        ));
        assert!(graph.edges().iter().any(|e|
            e.from == "src/types.rs::Widget" && e.to == "src/types.rs::Render" && e.kind == DependencyEdgeKind::StructTraitImpl
        ));
    }

    #[test]
    fn slice_traverses_two_hops_but_keeps_edge_directions() {
        let graph = DependencyGraph::new(vec![
            DependencyEdge::new("a", "b", DependencyEdgeKind::Calls).unwrap(),
            DependencyEdge::new("c", "a", DependencyEdgeKind::Calls).unwrap(),
            DependencyEdge::new("b", "d", DependencyEdgeKind::FunctionUsesType).unwrap(),
            DependencyEdge::new("d", "e", DependencyEdgeKind::Calls).unwrap(),
        ]).unwrap();
        let slice = two_hop_slice(&graph, "a", 8, 16).unwrap();
        assert_eq!(slice.nodes, vec!["b", "c", "d"]);
        assert!(slice.edges.iter().any(|e| e.from == "c" && e.to == "a"));
        assert!(!slice.nodes.contains(&"e".to_owned()));
    }

    #[test]
    fn prompt_context_is_bounded_and_stable() {
        let graph = DependencyGraph::new(vec![
            DependencyEdge::new("a", "b", DependencyEdgeKind::Calls).unwrap(),
        ]).unwrap();
        let slice = two_hop_slice(&graph, "a", 4, 4).unwrap();
        let one = slice.to_prompt_context(256).unwrap();
        assert_eq!(one, slice.to_prompt_context(256).unwrap());
        assert!(one.contains("calls: a -> b"));
        assert!(slice.to_prompt_context(8).is_err());
    }

    #[test]
    fn slice_limits_are_enforced() {
        let graph = DependencyGraph::new(vec![
            DependencyEdge::new("a", "b", DependencyEdgeKind::Calls).unwrap(),
        ]).unwrap();
        assert!(two_hop_slice(&graph, "a", 0, 4).is_err());
        assert!(two_hop_slice(&graph, "a", MAX_SLICE_NODES + 1, 4).is_err());
    }
}
