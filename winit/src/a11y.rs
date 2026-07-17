//! Bridge AccessKit adapter callbacks into the shell's event loop.
//!
//! The AccessKit adapter invokes its handlers from assistive-technology
//! threads; these bridges forward the requests through a channel drained by
//! `run_instance`, which owns the user interfaces the accessibility trees
//! are harvested from.
use crate::core::a11y::accesskit::{
    ActionRequest, ActivationHandler, DeactivationHandler, Node, NodeId, Role,
    Tree, TreeId, TreeUpdate,
};
use crate::core::window;
use crate::futures::futures::channel::mpsc;

/// An [`accesskit_winit::Adapter`] boxed with a [`Debug`] impl, so it can
/// travel inside the shell's event types.
pub struct Adapter(pub Box<accesskit_winit::Adapter>);

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
    /// Where requests are forwarded.
    pub sender: mpsc::UnboundedSender<A11yEvent>,
}

impl crate::core::a11y::accesskit::ActionHandler for ActionBridge {
    fn do_action(&mut self, request: ActionRequest) {
        let _ = self.sender.unbounded_send(A11yEvent::Action {
            window: self.window,
            request,
        });
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
