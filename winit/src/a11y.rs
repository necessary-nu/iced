//! Bridge AccessKit adapter callbacks into the shell's event loop.
//!
//! The AccessKit adapter invokes its handlers from assistive-technology
//! threads; these bridges forward the requests through a channel drained by
//! `run_instance`, which owns the user interfaces the accessibility trees
//! are harvested from.
use crate::core::a11y::accesskit::{
    Action, ActionRequest, ActivationHandler, DeactivationHandler, Node, NodeId, Role, Tree,
    TreeId, TreeUpdate,
};
use crate::core::window;
use crate::futures::futures::channel::mpsc;

/// An [`accesskit_winit::Adapter`] boxed with a [`Debug`] impl, so it can
/// travel inside the shell's event types.
pub struct Adapter(pub Box<accesskit_winit::Adapter>);

/// The AccessKit focus reported for one window. Focus requests update this
/// state before the next tree push; if the focused widget disappears during a
/// rebuild, [`Self::resolve`] falls back to the window root so every
/// `TreeUpdate` references a node that is actually present.
#[derive(Debug, Clone, Copy)]
pub(crate) struct FocusState {
    window_node: u64,
    current: NodeId,
}

impl FocusState {
    /// Starts with the window itself focused, matching the activation tree.
    pub(crate) fn new(window_node: u64) -> Self {
        Self {
            window_node,
            current: NodeId(window_node),
        }
    }

    /// The numeric window node used by iced's accessibility tree wrapper.
    pub(crate) fn window_node(self) -> u64 {
        self.window_node
    }

    /// Applies the focus effect of an AccessKit action request.
    pub(crate) fn handle_action(&mut self, action: Action, target: NodeId) {
        match action {
            Action::Focus => self.current = target,
            Action::Blur if self.current == target => self.current = NodeId(self.window_node),
            _ => {}
        }
    }

    /// Returns a valid focus for `nodes`, resetting stale widget focus to the
    /// window root after conditional content or a whole UI rebuild removes it.
    pub(crate) fn resolve(&mut self, nodes: &[(NodeId, Node)]) -> NodeId {
        if !nodes.iter().any(|(id, _)| *id == self.current) {
            self.current = NodeId(self.window_node);
        }
        self.current
    }
}

impl std::fmt::Debug for Adapter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("accesskit_winit::Adapter")
    }
}

impl std::ops::Deref for Adapter {
    type Target = accesskit_winit::Adapter;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl std::ops::DerefMut for Adapter {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

/// An accessibility request forwarded from an adapter handler.
#[derive(Debug)]
pub enum A11yEvent {
    /// Assistive technology connected to the window; full tree updates
    /// should be pushed from now on.
    Enabled {
        /// The window whose adapter was activated.
        window: window::Id,
    },
    /// Assistive technology disconnected from the window.
    Disabled {
        /// The window whose adapter was deactivated.
        window: window::Id,
    },
    /// Assistive technology requested an action on a node.
    Action {
        /// The window the request targets.
        window: window::Id,
        /// The requested action.
        request: ActionRequest,
    },
}

/// Builds the placeholder tree served synchronously on activation: the
/// window node alone, until the next redraw pushes the real tree.
pub fn window_tree(window_node: u64, title: &str) -> TreeUpdate {
    let mut node = Node::new(Role::Window);
    node.set_label(title);
    let root = NodeId(window_node);
    TreeUpdate {
        nodes: vec![(root, node)],
        tree: Some(Tree::new(root)),
        tree_id: TreeId::ROOT,
        focus: root,
    }
}

/// Serves the initial tree and reports activation to the event loop.
pub struct ActivationBridge {
    /// The window this adapter belongs to.
    pub window: window::Id,
    /// The window's accessibility node key.
    pub window_node: u64,
    /// The window title, served in the placeholder tree.
    pub title: String,
    /// The winit window, used to wake the event loop with a redraw so the
    /// real tree is pushed promptly after activation.
    pub raw: std::sync::Arc<winit::window::Window>,
    /// Where activation is reported.
    pub sender: mpsc::UnboundedSender<A11yEvent>,
}

impl ActivationHandler for ActivationBridge {
    fn request_initial_tree(&mut self) -> Option<TreeUpdate> {
        let _ = self.sender.unbounded_send(A11yEvent::Enabled {
            window: self.window,
        });
        self.raw.request_redraw();

        Some(window_tree(self.window_node, &self.title))
    }
}

/// Forwards action requests to the event loop.
pub struct ActionBridge {
    /// The window this adapter belongs to.
    pub window: window::Id,
    /// The winit window, used to wake the event loop so the action is handled
    /// promptly even when no visual event is pending.
    pub raw: std::sync::Arc<winit::window::Window>,
    /// Where requests are forwarded.
    pub sender: mpsc::UnboundedSender<A11yEvent>,
}

impl crate::core::a11y::accesskit::ActionHandler for ActionBridge {
    fn do_action(&mut self, request: ActionRequest) {
        let _ = self.sender.unbounded_send(A11yEvent::Action {
            window: self.window,
            request,
        });
        self.raw.request_redraw();
    }
}

/// Reports deactivation to the event loop.
pub struct DeactivationBridge {
    /// The window this adapter belongs to.
    pub window: window::Id,
    /// Where deactivation is reported.
    pub sender: mpsc::UnboundedSender<A11yEvent>,
}

impl DeactivationHandler for DeactivationBridge {
    fn deactivate_accessibility(&mut self) {
        let _ = self.sender.unbounded_send(A11yEvent::Disabled {
            window: self.window,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn focus_requests_survive_tree_updates_and_stale_focus_falls_back() {
        let root = NodeId(10);
        let control = NodeId(20);
        let mut focus = FocusState::new(10);
        let nodes = vec![
            (root, Node::new(Role::Window)),
            (control, Node::new(Role::Button)),
        ];

        focus.handle_action(Action::Focus, control);
        assert_eq!(focus.resolve(&nodes), control);

        focus.handle_action(Action::Blur, control);
        assert_eq!(focus.resolve(&nodes), root);

        focus.handle_action(Action::Focus, control);
        let root_only = vec![(root, Node::new(Role::Window))];
        assert_eq!(focus.resolve(&root_only), root);
    }
}
