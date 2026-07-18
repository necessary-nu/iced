use crate::core::a11y::accesskit::{
    Action, ActionData, ActionRequest, Affine, NodeId, Role, ScrollUnit, Toggled, TreeId,
};
use crate::core::a11y::{A11yNode, A11yTree, node_id};
use crate::core::{Element, Event, Font, Length, Size, event, mouse, shell, widget, window};
use crate::{Theme, text_editor};
use iced_runtime::user_interface::{Cache, UserInterface};

type TestElement<'a, Message> = Element<'a, Message, Theme, ()>;

const BOUNDS: Size = Size::new(400.0, 300.0);

fn harvest<Message>(
    interface: &mut UserInterface<'_, Message, Theme, ()>,
    renderer: &(),
) -> A11yTree {
    interface.a11y_nodes(mouse::Cursor::Unavailable, renderer)
}

fn node_with_role(tree: &A11yTree, role: Role) -> &A11yNode {
    tree.root()
        .iter()
        .chain(tree.children())
        .find(|node| node.node().role() == role)
        .unwrap_or_else(|| panic!("missing accessibility node with role {role:?}"))
}

fn nodes_with_role(tree: &A11yTree, role: Role) -> Vec<&A11yNode> {
    tree.root()
        .iter()
        .chain(tree.children())
        .filter(|node| node.node().role() == role)
        .collect()
}

fn dispatch<Message>(
    interface: &mut UserInterface<'_, Message, Theme, ()>,
    renderer: &mut (),
    target_node: NodeId,
    action: Action,
    data: Option<ActionData>,
) -> Vec<Message> {
    let event = Event::Accessibility(ActionRequest {
        action,
        target_tree: TreeId::ROOT,
        target_node,
        data,
    });
    let mut messages = Vec::new();
    let (_, statuses) = interface.update(
        &window::Headless,
        &shell::Waker::noop(),
        &[event],
        mouse::Cursor::Unavailable,
        renderer,
        &mut messages,
    );

    assert_eq!(statuses, [event::Status::Captured]);
    messages
}

#[derive(Debug, Clone, PartialEq)]
enum ControlMessage {
    Pressed,
    Checked(bool),
    Toggled(bool),
}

fn control_view(
    button_id: widget::Id,
    checkbox_id: widget::Id,
    toggler_id: widget::Id,
) -> TestElement<'static, ControlMessage> {
    let button: TestElement<'_, _> = crate::button(crate::text("Press"))
        .id(button_id)
        .on_press(ControlMessage::Pressed)
        .into();
    let checkbox: TestElement<'_, _> = crate::checkbox(false)
        .id(checkbox_id)
        .label("Check")
        .on_toggle(ControlMessage::Checked)
        .into();
    let toggler: TestElement<'_, _> = crate::toggler(false)
        .id(toggler_id)
        .label("Toggle")
        .on_toggle(ControlMessage::Toggled)
        .into();

    crate::column([button, checkbox, toggler]).into()
}

#[test]
fn generated_node_ids_survive_view_reconstruction() {
    fn view() -> TestElement<'static, ControlMessage> {
        let button: TestElement<'_, _> = crate::button(crate::text("Press"))
            .on_press(ControlMessage::Pressed)
            .into();
        let label: TestElement<'_, _> = crate::text("Status").into();

        crate::column([button, label]).into()
    }

    let mut renderer = ();
    let mut interface = UserInterface::build(view(), BOUNDS, Cache::new(), &mut renderer);
    let first = {
        let tree = harvest(&mut interface, &renderer);
        tree.root()
            .iter()
            .chain(tree.children())
            .map(|node| node.id().clone())
            .collect::<Vec<_>>()
    };
    let cache = interface.into_cache();

    let mut rebuilt = UserInterface::build(view(), BOUNDS, cache, &mut renderer);
    let second = {
        let tree = harvest(&mut rebuilt, &renderer);
        tree.root()
            .iter()
            .chain(tree.children())
            .map(|node| node.id().clone())
            .collect::<Vec<_>>()
    };

    assert_eq!(first, second);
}

#[test]
fn buttons_checkboxes_and_switches_expose_and_handle_actions() {
    let button_id = widget::Id::new("button");
    let checkbox_id = widget::Id::new("checkbox");
    let toggler_id = widget::Id::new("toggler");
    let mut renderer = ();
    let mut interface = UserInterface::build(
        control_view(button_id.clone(), checkbox_id.clone(), toggler_id.clone()),
        BOUNDS,
        Cache::new(),
        &mut renderer,
    );

    let tree = harvest(&mut interface, &renderer);
    let button = node_with_role(&tree, Role::Button).node();
    assert!(button.supports_action(Action::Click));
    assert!(button.supports_action(Action::Focus));

    let checkbox = node_with_role(&tree, Role::CheckBox).node();
    assert_eq!(checkbox.label(), Some("Check"));
    assert_eq!(checkbox.toggled(), Some(Toggled::False));
    assert!(checkbox.supports_action(Action::Click));

    let toggler = node_with_role(&tree, Role::Switch).node();
    assert_eq!(toggler.label(), Some("Toggle"));
    assert_eq!(toggler.toggled(), Some(Toggled::False));
    assert!(toggler.supports_action(Action::Click));

    assert_eq!(
        dispatch(
            &mut interface,
            &mut renderer,
            node_id(&button_id),
            Action::Click,
            None,
        ),
        [ControlMessage::Pressed]
    );
    assert_eq!(
        dispatch(
            &mut interface,
            &mut renderer,
            node_id(&checkbox_id),
            Action::Click,
            None,
        ),
        [ControlMessage::Checked(true)]
    );
    assert_eq!(
        dispatch(
            &mut interface,
            &mut renderer,
            node_id(&toggler_id),
            Action::Click,
            None,
        ),
        [ControlMessage::Toggled(true)]
    );
}

#[test]
fn disabled_controls_do_not_advertise_actions() {
    let button: TestElement<'_, ControlMessage> = crate::button(crate::text("Disabled")).into();
    let checkbox: TestElement<'_, ControlMessage> = crate::checkbox(false).label("Disabled").into();
    let toggler: TestElement<'_, ControlMessage> = crate::toggler(false).label("Disabled").into();
    let mut renderer = ();
    let mut interface = UserInterface::build(
        crate::column([button, checkbox, toggler]),
        BOUNDS,
        Cache::new(),
        &mut renderer,
    );

    let tree = harvest(&mut interface, &renderer);
    for role in [Role::Button, Role::CheckBox, Role::Switch] {
        let node = node_with_role(&tree, role).node();
        assert!(node.is_disabled());
        assert!(!node.supports_action(Action::Click));
        assert!(!node.supports_action(Action::Focus));
    }
}

#[derive(Debug, Clone, PartialEq)]
enum SliderMessage {
    Changed(f64),
}

#[test]
fn slider_exposes_numeric_semantics_and_handles_value_actions() {
    let id = widget::Id::new("slider");
    let slider: TestElement<'_, SliderMessage> =
        crate::slider(0.0..=10.0, 4.0, SliderMessage::Changed)
            .id(id.clone())
            .step(2.0)
            .into();
    let mut renderer = ();
    let mut interface = UserInterface::build(slider, BOUNDS, Cache::new(), &mut renderer);

    let tree = harvest(&mut interface, &renderer);
    let node = node_with_role(&tree, Role::Slider).node();
    assert_eq!(node.numeric_value(), Some(4.0));
    assert_eq!(node.min_numeric_value(), Some(0.0));
    assert_eq!(node.max_numeric_value(), Some(10.0));
    assert_eq!(node.numeric_value_step(), Some(2.0));
    for action in [Action::SetValue, Action::Increment, Action::Decrement] {
        assert!(node.supports_action(action));
    }

    assert_eq!(
        dispatch(
            &mut interface,
            &mut renderer,
            node_id(&id),
            Action::SetValue,
            Some(ActionData::NumericValue(8.8)),
        ),
        [SliderMessage::Changed(8.0)]
    );
    assert_eq!(
        dispatch(
            &mut interface,
            &mut renderer,
            node_id(&id),
            Action::Increment,
            None,
        ),
        [SliderMessage::Changed(10.0)]
    );
}

#[derive(Debug, Clone, PartialEq)]
enum InputMessage {
    Changed(String),
}

#[test]
fn text_input_distinguishes_disabled_and_secure_content_and_handles_edits() {
    let id = widget::Id::new("input");
    let input: TestElement<'_, InputMessage> = crate::text_input("Placeholder", "old")
        .id(id.clone())
        .on_input(InputMessage::Changed)
        .into();
    let mut renderer = ();
    let mut interface = UserInterface::build(input, BOUNDS, Cache::new(), &mut renderer);

    let tree = harvest(&mut interface, &renderer);
    let node = node_with_role(&tree, Role::TextInput).node();
    assert_eq!(node.value(), Some("old"));
    assert!(!node.is_disabled());
    assert!(node.supports_action(Action::SetValue));
    assert!(node.supports_action(Action::ReplaceSelectedText));

    assert_eq!(
        dispatch(
            &mut interface,
            &mut renderer,
            node_id(&id),
            Action::SetValue,
            Some(ActionData::Value("new value".into())),
        ),
        [InputMessage::Changed("new value".into())]
    );

    let replacement_id = widget::Id::new("replacement-input");
    let replacement: TestElement<'_, InputMessage> = crate::text_input("", "old")
        .id(replacement_id.clone())
        .on_input(InputMessage::Changed)
        .into();
    let mut replacement_interface =
        UserInterface::build(replacement, BOUNDS, Cache::new(), &mut renderer);
    assert_eq!(
        dispatch(
            &mut replacement_interface,
            &mut renderer,
            node_id(&replacement_id),
            Action::ReplaceSelectedText,
            Some(ActionData::Value("X".into())),
        ),
        [InputMessage::Changed("Xold".into())]
    );

    let disabled: TestElement<'_, InputMessage> = crate::text_input("Placeholder", "").into();
    let mut disabled_interface =
        UserInterface::build(disabled, BOUNDS, Cache::new(), &mut renderer);
    let disabled_tree = harvest(&mut disabled_interface, &renderer);
    let disabled_node = node_with_role(&disabled_tree, Role::TextInput).node();
    assert_eq!(disabled_node.placeholder(), Some("Placeholder"));
    assert!(disabled_node.is_disabled());
    assert!(!disabled_node.is_read_only());
    assert!(!disabled_node.supports_action(Action::SetValue));

    let secure: TestElement<'_, InputMessage> = crate::text_input("", "secret")
        .secure(true)
        .on_input(InputMessage::Changed)
        .into();
    let mut secure_interface = UserInterface::build(secure, BOUNDS, Cache::new(), &mut renderer);
    let secure_tree = harvest(&mut secure_interface, &renderer);
    let secure_node = node_with_role(&secure_tree, Role::PasswordInput).node();
    assert_ne!(secure_node.value(), Some("secret"));
    assert_eq!(secure_node.value().unwrap().chars().count(), 6);
}

#[derive(Debug, Clone)]
enum EditorMessage {
    Action(text_editor::Action),
}

#[test]
fn text_editor_exposes_multiline_semantics_and_handles_replacement_actions() {
    let id = widget::Id::new("editor");
    // The null renderer intentionally discards editor contents, so exercise
    // the empty-content/placeholder branch while keeping the test headless.
    let content = text_editor::Content::<()>::new();
    let editor: TestElement<'_, EditorMessage> = crate::text_editor(&content)
        .id(id.clone())
        .placeholder("Write here")
        .on_action(EditorMessage::Action)
        .into();
    let mut renderer = ();
    let mut interface = UserInterface::build(editor, BOUNDS, Cache::new(), &mut renderer);

    let tree = harvest(&mut interface, &renderer);
    let node = node_with_role(&tree, Role::MultilineTextInput).node();
    assert_eq!(node.placeholder(), Some("Write here"));
    assert!(node.supports_action(Action::SetValue));
    assert!(node.supports_action(Action::ReplaceSelectedText));

    let messages = dispatch(
        &mut interface,
        &mut renderer,
        node_id(&id),
        Action::SetValue,
        Some(ActionData::Value("replacement".into())),
    );
    assert_eq!(messages.len(), 2);
    assert!(matches!(
        messages[0],
        EditorMessage::Action(text_editor::Action::SelectAll)
    ));
    assert!(matches!(
        &messages[1],
        EditorMessage::Action(text_editor::Action::Edit(text_editor::Edit::Paste(value)))
            if value.as_str() == "replacement"
    ));
}

#[derive(Debug, Clone, PartialEq)]
enum PickMessage {
    Opened,
    Closed,
    Selected(i32),
}

#[test]
fn pick_list_exposes_combo_box_state_and_handles_selection_actions() {
    let id = widget::Id::new("pick-list");
    let pick_list: TestElement<'_, PickMessage> =
        crate::pick_list(Some(1), [1, 2, 3], |value| value.to_string())
            .id(id.clone())
            .on_select(PickMessage::Selected)
            .on_open(PickMessage::Opened)
            .on_close(PickMessage::Closed)
            .into();
    let mut renderer = ();
    let mut interface = UserInterface::build(pick_list, BOUNDS, Cache::new(), &mut renderer);

    let tree = harvest(&mut interface, &renderer);
    let node = node_with_role(&tree, Role::ComboBox).node();
    assert_eq!(node.value(), Some("1"));
    assert_eq!(node.is_expanded(), Some(false));
    assert!(node.supports_action(Action::Expand));
    assert!(node.supports_action(Action::SetValue));

    assert_eq!(
        dispatch(
            &mut interface,
            &mut renderer,
            node_id(&id),
            Action::Expand,
            None,
        ),
        [PickMessage::Opened]
    );
    let expanded_tree = harvest(&mut interface, &renderer);
    let expanded = node_with_role(&expanded_tree, Role::ComboBox).node();
    assert_eq!(expanded.is_expanded(), Some(true));
    assert!(expanded.supports_action(Action::Collapse));

    assert_eq!(
        dispatch(
            &mut interface,
            &mut renderer,
            node_id(&id),
            Action::SetValue,
            Some(ActionData::Value("3".into())),
        ),
        [PickMessage::Selected(3)]
    );
}

#[test]
fn plain_rich_and_layout_text_remain_accessible() {
    let plain: TestElement<'_, ()> = crate::text("Plain text").into();
    let rich: TestElement<'_, ()> = crate::rich_text::<(), (), Theme, ()>([
        crate::span::<(), Font>("Rich "),
        crate::span::<(), Font>("text"),
    ])
    .into();
    let nested: TestElement<'_, ()> = crate::container(crate::row([
        crate::text("Nested one").into(),
        crate::text("Nested two").into(),
    ]))
    .into();
    let mut renderer = ();
    let mut interface = UserInterface::build(
        crate::column([plain, rich, nested]),
        BOUNDS,
        Cache::new(),
        &mut renderer,
    );

    let tree = harvest(&mut interface, &renderer);
    let values = nodes_with_role(&tree, Role::Label)
        .into_iter()
        .filter_map(|node| node.node().value())
        .collect::<Vec<_>>();

    assert!(values.contains(&"Plain text"));
    assert!(values.contains(&"Rich text"));
    assert!(values.contains(&"Nested one"));
    assert!(values.contains(&"Nested two"));
}

#[derive(Debug, Clone, PartialEq)]
enum ScrollMessage {
    Scrolled,
}

#[test]
fn scrollable_exposes_ranges_and_handles_accessibility_scrolling() {
    let id = widget::Id::new("scrollable");
    let content: TestElement<'_, ScrollMessage> = crate::text("Scrollable child")
        .height(Length::Fixed(300.0))
        .into();
    let scrollable: TestElement<'_, ScrollMessage> = crate::scrollable(content)
        .id(id.clone())
        .height(Length::Fixed(100.0))
        .on_scroll(|_| ScrollMessage::Scrolled)
        .into();
    let mut renderer = ();
    let mut interface = UserInterface::build(
        scrollable,
        Size::new(200.0, 100.0),
        Cache::new(),
        &mut renderer,
    );

    let tree = harvest(&mut interface, &renderer);
    let node = node_with_role(&tree, Role::ScrollView).node();
    assert_eq!(node.scroll_y(), Some(0.0));
    assert_eq!(node.scroll_y_min(), Some(0.0));
    assert_eq!(node.scroll_y_max(), Some(200.0));
    assert!(node.supports_action(Action::ScrollDown));
    assert!(node.supports_action(Action::SetScrollOffset));

    assert_eq!(
        dispatch(
            &mut interface,
            &mut renderer,
            node_id(&id),
            Action::ScrollDown,
            Some(ActionData::ScrollUnit(ScrollUnit::Item)),
        ),
        [ScrollMessage::Scrolled]
    );

    let scrolled_tree = harvest(&mut interface, &renderer);
    let scroll_node = node_with_role(&scrolled_tree, Role::ScrollView).node();
    assert_eq!(scroll_node.scroll_y(), Some(60.0));
    let child = node_with_role(&scrolled_tree, Role::Label).node();
    assert_ne!(
        child.transform().copied().unwrap_or(Affine::IDENTITY),
        Affine::IDENTITY
    );
}

#[cfg(all(feature = "image", feature = "svg"))]
#[test]
fn media_is_decorative_by_default_and_labeled_on_request() {
    let decorative_image: TestElement<'_, ()> =
        crate::image(crate::core::image::Handle::from_bytes(vec![0])).into();
    let labeled_image: TestElement<'_, ()> =
        crate::image(crate::core::image::Handle::from_bytes(vec![1]))
            .alt_text("Raster description")
            .into();
    let decorative_svg: TestElement<'_, ()> =
        crate::svg(crate::core::svg::Handle::from_memory(b"<svg/>"))
            .height(Length::Fixed(20.0))
            .into();
    let labeled_svg: TestElement<'_, ()> =
        crate::svg(crate::core::svg::Handle::from_memory(b"<svg/>"))
            .height(Length::Fixed(20.0))
            .alt_text("Vector description")
            .into();
    let mut renderer = ();
    let mut interface = UserInterface::build(
        crate::column([decorative_image, labeled_image, decorative_svg, labeled_svg]),
        BOUNDS,
        Cache::new(),
        &mut renderer,
    );

    let tree = harvest(&mut interface, &renderer);
    let labels = nodes_with_role(&tree, Role::Image)
        .into_iter()
        .filter_map(|node| node.node().label())
        .collect::<Vec<_>>();
    assert_eq!(labels, ["Raster description", "Vector description"]);
}
