//! Accessibility trees for widgets, bridged to platform accessibility APIs
//! through [AccessKit](https://accesskit.dev).
//!
//! Widgets describe themselves by returning an [`A11yTree`] from
//! [`Widget::a11y_nodes`]; the shell assembles the per-window tree and hands
//! it to the AccessKit adapter.
//!
//! Adapted from the `iced_accessibility` crate of the pop-os/iced fork,
//! simplified against upstream's `widget::Id` (which stays untouched; see
//! [`Id::a11y_key`] for the stable numeric mapping).
//!
//! [`Widget::a11y_nodes`]: crate::Widget::a11y_nodes
//! [`Id::a11y_key`]: crate::widget::Id::a11y_key

use std::sync::atomic::{self, AtomicU64};

pub use accesskit;

use crate::widget::Id;

/// The identity of a node in the accessibility tree.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum A11yId {
    /// A window node.
    Window(u64),
    /// A widget node, keyed by the widget's [`Id`].
    Widget(Id),
}

impl From<u64> for A11yId {
    fn from(id: u64) -> Self {
        Self::Window(id)
    }
}

impl From<Id> for A11yId {
    fn from(id: Id) -> Self {
        Self::Widget(id)
    }
}

impl From<&A11yId> for accesskit::NodeId {
    fn from(value: &A11yId) -> Self {
        let key = match value {
            // Window keys live in `u32::MAX..u32::MAX + 2^32`.
            A11yId::Window(id) => *id,
            A11yId::Widget(id) => id.a11y_key(),
        };
        accesskit::NodeId(key)
    }
}

impl From<A11yId> for accesskit::NodeId {
    fn from(value: A11yId) -> Self {
        Self::from(&value)
    }
}

static NEXT_WINDOW_ID: AtomicU64 = AtomicU64::new(1);

/// Allocates a window node key that cannot collide with widget keys for the
/// duration of the program.
pub fn window_node_id() -> u64 {
    u64::from(u32::MAX) + NEXT_WINDOW_ID.fetch_add(1, atomic::Ordering::Relaxed)
}

/// An accessibility node paired with its identity.
#[derive(Debug, Clone, PartialEq)]
pub struct A11yNode {
    node: accesskit::Node,
    id: A11yId,
}

impl A11yNode {
    /// Creates a new [`A11yNode`].
    pub fn new<T: Into<A11yId>>(node: accesskit::Node, id: T) -> Self {
        Self {
            node,
            id: id.into(),
        }
    }

    /// The identity of this node.
    pub fn id(&self) -> &A11yId {
        &self.id
    }

    /// The underlying AccessKit node.
    pub fn node(&self) -> &accesskit::Node {
        &self.node
    }

    /// The underlying AccessKit node, mutably.
    pub fn node_mut(&mut self) -> &mut accesskit::Node {
        &mut self.node
    }

    /// Prepends the given identities to this node's children.
    pub fn add_children(&mut self, children: Vec<A11yId>) {
        let mut children = children
            .iter()
            .map(accesskit::NodeId::from)
            .collect::<Vec<_>>();
        children.extend_from_slice(self.node.children());
        self.node.set_children(children);
    }
}

impl From<A11yNode> for (accesskit::NodeId, accesskit::Node) {
    fn from(node: A11yNode) -> Self {
        ((&node.id).into(), node.node)
    }
}

/// The accessibility tree of a widget and its children.
#[derive(Debug, Clone, Default)]
pub struct A11yTree {
    /// The root nodes of the current widget: children of the parent widget,
    /// or of the window if there is no parent.
    root: Vec<A11yNode>,
    /// The nodes of the widget's descendants.
    children: Vec<A11yNode>,
}

impl A11yTree {
    /// Creates a new [`A11yTree`] from raw parts.
    ///
    /// The caller is responsible for the child relationships of the root
    /// nodes.
    pub fn new(root: Vec<A11yNode>, children: Vec<A11yNode>) -> Self {
        Self { root, children }
    }

    /// A tree consisting of a single node with no children.
    pub fn leaf<T: Into<A11yId>>(node: accesskit::Node, id: T) -> Self {
        Self {
            root: vec![A11yNode::new(node, id)],
            children: vec![],
        }
    }

    /// A tree with a single root node adopting the given child tree.
    pub fn node_with_child_tree(mut root: A11yNode, child_tree: Self) -> Self {
        root.add_children(
            child_tree.root.iter().map(A11yNode::id).cloned().collect(),
        );
        Self {
            root: vec![root],
            children: child_tree
                .children
                .into_iter()
                .chain(child_tree.root)
                .collect(),
        }
    }

    /// Joins multiple trees into a single tree.
    pub fn join<T: Iterator<Item = Self>>(trees: T) -> Self {
        trees.fold(Self::default(), |mut acc, A11yTree { root, children }| {
            acc.root.extend(root);
            acc.children.extend(children);
            acc
        })
    }

    /// The root nodes of this tree.
    pub fn root(&self) -> &Vec<A11yNode> {
        &self.root
    }

    /// The descendant nodes of this tree.
    pub fn children(&self) -> &Vec<A11yNode> {
        &self.children
    }

    /// The root nodes of this tree, mutably.
    pub fn root_mut(&mut self) -> &mut Vec<A11yNode> {
        &mut self.root
    }

    /// The descendant nodes of this tree, mutably.
    pub fn children_mut(&mut self) -> &mut Vec<A11yNode> {
        &mut self.children
    }

    /// Whether the tree contains a node with the given identity.
    pub fn contains(&self, id: &A11yId) -> bool {
        self.root.iter().any(|n| n.id() == id)
            || self.children.iter().any(|n| n.id() == id)
    }
}

impl From<A11yTree> for Vec<(accesskit::NodeId, accesskit::Node)> {
    fn from(tree: A11yTree) -> Vec<(accesskit::NodeId, accesskit::Node)> {
        tree.root
            .into_iter()
            .map(A11yNode::into)
            .chain(tree.children.into_iter().map(A11yNode::into))
            .collect()
    }
}
