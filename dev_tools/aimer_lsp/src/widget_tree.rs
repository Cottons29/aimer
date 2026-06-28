use serde::{Deserialize, Serialize};

use crate::widget_analyzer::WidgetInfo;

/// A node in the widget tree, sent to the IDE.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WidgetTreeNode {
    /// Widget name (struct/enum name).
    pub name: String,
    /// Widget type: "stateless", "stateful", "router", "rawWidget".
    pub kind: String,
    /// File URI.
    pub file_uri: String,
    /// Line number (0-based).
    pub line: u32,
    /// Child widgets (empty for leaf nodes).
    pub children: Vec<WidgetTreeNode>,
}

/// Build a flat widget tree from all analyzed widgets.
///
/// Since we can't resolve type references without a full Rust compiler,
/// this returns a flat list grouped by file. The IDE can build the
/// hierarchy by matching widget types to container children.
pub fn build_widget_tree(widgets: &[WidgetInfo]) -> Vec<WidgetTreeNode> {
    widgets
        .iter()
        .map(|w| WidgetTreeNode {
            name: w.name.clone(),
            kind: w.kind.label().to_lowercase(),
            file_uri: w.file_uri.clone(),
            line: w.line,
            children: vec![],
        })
        .collect()
}
