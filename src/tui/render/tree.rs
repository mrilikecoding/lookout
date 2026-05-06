use crate::card::TreeNode;
use ratatui::layout::Rect;
use ratatui::widgets::{Block, Borders};
use ratatui::Frame;
use tui_tree_widget::{Tree, TreeItem, TreeState};

/// Build a TreeItem using the child index as sibling-scoped identifier.
/// tui-tree-widget requires uniqueness only among siblings, not globally.
fn build_items(node: &TreeNode) -> TreeItem<'_, usize> {
    if node.children.is_empty() {
        TreeItem::new_leaf(0, node.label.as_str())
    } else {
        let children: Vec<TreeItem<usize>> = node
            .children
            .iter()
            .enumerate()
            .map(|(i, c)| build_child(i, c))
            .collect();
        TreeItem::new(0, node.label.as_str(), children)
            .expect("indices are unique by construction")
    }
}

fn build_child(index: usize, node: &TreeNode) -> TreeItem<'_, usize> {
    if node.children.is_empty() {
        TreeItem::new_leaf(index, node.label.as_str())
    } else {
        let children: Vec<TreeItem<usize>> = node
            .children
            .iter()
            .enumerate()
            .map(|(i, c)| build_child(i, c))
            .collect();
        TreeItem::new(index, node.label.as_str(), children)
            .expect("indices are unique by construction")
    }
}

pub fn render(f: &mut Frame, area: Rect, root: &TreeNode) {
    let items = vec![build_items(root)];
    let mut state = TreeState::default();
    let tree = Tree::new(&items)
        .expect("single root item, no duplicates")
        .block(Block::default().borders(Borders::ALL).title("Tree"));
    f.render_stateful_widget(tree, area, &mut state);
}
