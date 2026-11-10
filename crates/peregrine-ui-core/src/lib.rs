//! Peregrine UI core: the timeline model and layout engine shared by every
//! frontend (catalog-2 track F). Frontends render; this crate owns the
//! zero-reflow contract: bounding boxes are computed once per event and
//! viewports anchor to event IDs, never to indices or pixels.

/// A laid-out event: stable identity plus geometry. Anchors reference
/// `event_id`, so backfills and edits never displace the reading position
/// (catalog-3 L-track: event-ID anchor pinning, focal-point resize).
#[derive(Debug, Clone, PartialEq)]
pub struct LayoutNode {
    /// Stable event identity (protocol event id, interned by the core).
    pub event_id: u64,
    /// Top edge relative to timeline origin, in pixels.
    pub top: f32,
    /// Total laid-out height, in pixels.
    pub height: f32,
}

impl LayoutNode {
    /// Bottom edge relative to timeline origin.
    #[must_use]
    pub const fn bottom(&self) -> f32 {
        self.top + self.height
    }
}

/// Linear timeline layout: node `n` starts exactly where node `n-1` ends.
/// No second pass, no reflow: total height is the sum, computed once.
#[derive(Debug, Default, Clone)]
pub struct TimelineLayout {
    nodes: Vec<LayoutNode>,
    total_height: f32,
}

impl TimelineLayout {
    /// Empty layout.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            nodes: Vec::new(),
            total_height: 0.0,
        }
    }

    /// Append a node at the running offset; O(1), no invalidation of
    /// earlier geometry.
    pub fn push(&mut self, event_id: u64, height: f32) {
        let top = self.total_height;
        self.total_height += height;
        self.nodes.push(LayoutNode {
            event_id,
            top,
            height,
        });
    }

    /// The node pinned by anchor: the node containing `y_viewport`, or the
    /// last node ending before it. This is the resize-pinning primitive.
    #[must_use]
    pub fn anchor_at(&self, y_viewport: f32) -> Option<&LayoutNode> {
        self.nodes
            .iter()
            .find(|n| n.bottom() > y_viewport)
            .or_else(|| self.nodes.last())
    }

    /// Total laid-out height.
    #[must_use]
    pub const fn total_height(&self) -> f32 {
        self.total_height
    }

    /// Node count.
    #[must_use]
    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    /// True when empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }
}

#[cfg(test)]
#[allow(clippy::expect_used)] // tests: panics are the assertion mechanism
mod tests {
    use super::*;

    #[test]
    fn layout_is_contiguous_and_reflow_free() {
        let mut l = TimelineLayout::new();
        l.push(1, 40.0);
        l.push(2, 120.0);
        l.push(3, 30.0);
        assert!((l.total_height() - 190.0).abs() < f32::EPSILON);
        let mid = l.anchor_at(100.0).expect("anchor inside layout");
        assert_eq!(mid.event_id, 2);
        assert!((mid.top - 40.0).abs() < f32::EPSILON);
    }

    #[test]
    fn anchor_past_end_pins_last_node() {
        let mut l = TimelineLayout::new();
        l.push(1, 10.0);
        assert_eq!(l.anchor_at(9999.0).expect("last").event_id, 1);
    }
}
