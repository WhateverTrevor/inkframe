//! Where nodes sit in the node view, saved with the project.
//!
//! The node view (in the app) is drawn from the layers every frame, so the
//! graph itself is never stored; only the places the person has dragged
//! nodes to are, so the view opens as it was left. Nodes nobody has moved
//! aren't listed and are laid out automatically. A place for a node that's
//! gone (its layer deleted, its filter removed) is simply never used, and is
//! dropped the next time the view is arranged.

use serde::{Deserialize, Serialize};

use crate::{Document, LayerId, Point, SymbolId, TimelineId};

/// A node in the node view, by what it stands for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum NodeRef {
    Layer(LayerId),
    /// A layer's colour effect.
    ColorEffect(LayerId),
    /// A layer's filter, by its place in the list.
    Filter(LayerId, usize),
    Composite,
    Display,
}

/// Where one node has been moved to, in the node view's own units, on the
/// main timeline's graph or a symbol's.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct NodePlace {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub symbol: Option<SymbolId>,
    pub node: NodeRef,
    pub at: Point,
}

fn symbol_of(timeline: TimelineId) -> Option<SymbolId> {
    match timeline {
        TimelineId::Main => None,
        TimelineId::Symbol(symbol) => Some(symbol),
    }
}

impl Document {
    /// Where `node` has been moved to on `timeline`'s graph, if it has.
    pub fn node_place(&self, timeline: TimelineId, node: NodeRef) -> Option<Point> {
        let symbol = symbol_of(timeline);
        self.node_places.iter().find(|p| p.symbol == symbol && p.node == node).map(|p| p.at)
    }

    /// Notes that `node` has been moved to `at` on `timeline`'s graph.
    pub fn set_node_place(&mut self, timeline: TimelineId, node: NodeRef, at: Point) {
        let symbol = symbol_of(timeline);
        match self.node_places.iter_mut().find(|p| p.symbol == symbol && p.node == node) {
            Some(place) => place.at = at,
            None => self.node_places.push(NodePlace { symbol, node, at }),
        }
    }

    /// Forgets where `timeline`'s nodes were moved, so they're laid out
    /// automatically again.
    pub fn clear_node_places(&mut self, timeline: TimelineId) {
        let symbol = symbol_of(timeline);
        self.node_places.retain(|p| p.symbol != symbol);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Transform;

    #[test]
    fn places_are_kept_per_graph_and_saved() {
        let mut doc = Document::default();
        let ball = doc.add_layer("Ball", Vec::new(), 0, Transform::default());
        doc.set_node_place(TimelineId::Main, NodeRef::Layer(ball), Point::new(10.0, 20.0));
        doc.set_node_place(TimelineId::Main, NodeRef::Layer(ball), Point::new(30.0, 40.0));
        doc.set_node_place(TimelineId::Main, NodeRef::Composite, Point::new(300.0, 0.0));
        assert_eq!(doc.node_places.len(), 2, "moved again, not added again");
        assert_eq!(doc.node_place(TimelineId::Main, NodeRef::Layer(ball)), Some(Point::new(30.0, 40.0)));

        let json = serde_json::to_string(&doc).unwrap();
        let loaded: Document = serde_json::from_str(&json).unwrap();
        assert_eq!(loaded.node_place(TimelineId::Main, NodeRef::Composite), Some(Point::new(300.0, 0.0)));

        doc.clear_node_places(TimelineId::Main);
        assert!(doc.node_places.is_empty());
        // Nothing moved, nothing written.
        assert!(!serde_json::to_string(&doc).unwrap().contains("node_places"));
    }
}
