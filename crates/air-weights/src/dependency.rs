//! Optional, versioned dependency graph stored as an AIRW v1 section.
//! Legacy AIRW readers ignore this section; readers that understand it validate
//! and decode it on demand so the archive remains mmap-backed by default.
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

pub const DEPENDENCY_GRAPH_SECTION: &str = "dependency-graph.v1";
pub const DEPENDENCY_GRAPH_KIND: &str = "dependency-graph-v1";
pub const MAX_DEPENDENCY_EDGES: usize = 65_536;
pub const MAX_DEPENDENCY_ID_BYTES: usize = 512;
pub const MAX_DEPENDENCY_SECTION_BYTES: usize = 16 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DependencyEdgeKind {
    /// A caller invokes a callee.
    Calls,
    /// A struct has an explicit trait implementation.
    StructTraitImpl,
    /// A function signature or body refers to a type.
    FunctionUsesType,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DependencyEdge {
    pub from: String,
    pub to: String,
    pub kind: DependencyEdgeKind,
}

impl DependencyEdge {
    pub fn new(
        from: impl Into<String>,
        to: impl Into<String>,
        kind: DependencyEdgeKind,
    ) -> Result<Self, &'static str> {
        let edge = Self { from: from.into(), to: to.into(), kind };
        edge.validate()?;
        Ok(edge)
    }

    fn validate(&self) -> Result<(), &'static str> {
        if !valid_symbol_id(&self.from) || !valid_symbol_id(&self.to) {
            return Err("dependency symbol identifier");
        }
        Ok(())
    }
}

/// Deterministically ordered typed edges. The section is optional in AIRW v1;
/// archives without it remain valid and are returned as having no graph.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DependencyGraph {
    pub edges: Vec<DependencyEdge>,
}

impl DependencyGraph {
    pub fn new(mut edges: Vec<DependencyEdge>) -> Result<Self, &'static str> {
        if edges.len() > MAX_DEPENDENCY_EDGES {
            return Err("dependency edge count");
        }
        for edge in &edges {
            edge.validate()?;
        }
        edges.sort_unstable();
        edges.dedup();
        Ok(Self { edges })
    }

    pub fn edges(&self) -> &[DependencyEdge] {
        &self.edges
    }

    pub fn validate(&self) -> Result<(), &'static str> {
        if self.edges.len() > MAX_DEPENDENCY_EDGES {
            return Err("dependency edge count");
        }
        let mut previous: Option<&DependencyEdge> = None;
        for edge in &self.edges {
            edge.validate()?;
            if previous.is_some_and(|p| p >= edge) {
                return Err("dependency edges must be sorted and unique");
            }
            previous = Some(edge);
        }
        Ok(())
    }

    /// Encode the graph for use as the optional AIRW v1 graph section.
    pub fn to_section_bytes(&self) -> Result<Vec<u8>, String> {
        self.validate().map_err(str::to_owned)?;
        let bytes = serde_json::to_vec(self).map_err(|e| e.to_string())?;
        if bytes.len() > MAX_DEPENDENCY_SECTION_BYTES {
            return Err("dependency graph section too large".into());
        }
        Ok(bytes)
    }

    /// Decode a graph section with the same bounds used by the archive reader.
    pub fn from_section_bytes(bytes: &[u8]) -> Result<Self, String> {
        if bytes.len() > MAX_DEPENDENCY_SECTION_BYTES {
            return Err("dependency graph section too large".into());
        }
        let graph: Self = serde_json::from_slice(bytes).map_err(|e| e.to_string())?;
        graph.validate().map_err(str::to_owned)?;
        Ok(graph)
    }
}

/// Store the graph as an optional, versioned AIRW payload section.
/// The container header and AIRW v1 compatibility remain unchanged.
pub fn add_to_archive(builder: &mut crate::ArchiveBuilder, graph: &DependencyGraph)
    -> Result<(), crate::ArchiveError>
{
    let bytes = graph.to_section_bytes()
        .map_err(|_| crate::ArchiveError::Invalid("dependency graph"))?;
    builder.add_section(DEPENDENCY_GRAPH_SECTION, DEPENDENCY_GRAPH_KIND, &bytes)
}

/// Read a graph section on demand. Legacy AIRW v1 archives return None.
pub fn read_from_archive(reader: &crate::ArchiveReader)
    -> Result<Option<DependencyGraph>, crate::ArchiveError>
{
    let Some(bytes) = reader.section(DEPENDENCY_GRAPH_SECTION) else {
        return Ok(None);
    };
    DependencyGraph::from_section_bytes(bytes)
        .map(Some)
        .map_err(|_| crate::ArchiveError::Invalid("dependency graph section"))
}

fn valid_symbol_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= MAX_DEPENDENCY_ID_BYTES
        && !id.chars().any(|c| c.is_control() || c.is_whitespace())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn graph_sorts_and_deduplicates_edges() {
        let b = DependencyEdge::new("src/b.rs::b", "src/a.rs::a", DependencyEdgeKind::Calls).unwrap();
        let a = DependencyEdge::new("src/a.rs::a", "src/b.rs::b", DependencyEdgeKind::Calls).unwrap();
        let graph = DependencyGraph::new(vec![b.clone(), a.clone(), b]).unwrap();
        assert_eq!(graph.edges(), &[a, DependencyEdge::new(
            "src/b.rs::b", "src/a.rs::a", DependencyEdgeKind::Calls
        ).unwrap()]);
    }

    #[test]
    fn graph_section_round_trips_and_rejects_bad_ids() {
        let graph = DependencyGraph::new(vec![
            DependencyEdge::new("src/lib.rs::Item", "core::fmt::Display", DependencyEdgeKind::StructTraitImpl).unwrap(),
        ]).unwrap();
        let bytes = graph.to_section_bytes().unwrap();
        assert_eq!(DependencyGraph::from_section_bytes(&bytes).unwrap(), graph);
        assert!(DependencyEdge::new("bad id", "good", DependencyEdgeKind::Calls).is_err());
    }

    #[test]
    fn optional_airw_section_round_trips_and_preserves_legacy_archives() {
        let graph = DependencyGraph::new(vec![
            DependencyEdge::new("src/a.rs::a", "src/b.rs::b", DependencyEdgeKind::Calls).unwrap(),
        ]).unwrap();
        let mut builder = crate::ArchiveBuilder::new("graph-demo");
        builder.add_language("rust").unwrap();
        add_to_archive(&mut builder, &graph).unwrap();
        let archive = crate::ArchiveReader::from_bytes(builder.finish().unwrap()).unwrap();
        assert_eq!(read_from_archive(&archive).unwrap(), Some(graph));

        let mut legacy = crate::ArchiveBuilder::new("legacy");
        legacy.add_language("rust").unwrap();
        legacy.add_section("rules", "rule-set", b"old-v1").unwrap();
        let archive = crate::ArchiveReader::from_bytes(legacy.finish().unwrap()).unwrap();
        assert_eq!(read_from_archive(&archive).unwrap(), None);
    }

    #[test]
    fn rejects_unsorted_or_duplicate_decoded_graphs() {
        let bytes = br#"{"edges":[{"from":"z","to":"a","kind":"calls"},{"from":"a","to":"z","kind":"calls"}]}"#;
        assert!(DependencyGraph::from_section_bytes(bytes).is_err());
    }
}
