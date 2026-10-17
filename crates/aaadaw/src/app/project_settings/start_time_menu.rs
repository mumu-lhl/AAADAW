use super::super::Message;
use iced::advanced::widget::{Operation, Tree, tree};
use iced::advanced::{Clipboard, Layout, Shell, Widget, layout, mouse, renderer};
use iced::widget::overlay::menu;
use iced::{Element, Event, Length, Theme};
use std::fmt;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum StartTimeCommand {
    SetTimeAtCursor,
    ResetTime,
    SetMeasureAtCursor,
    ResetMeasure,
    AllowFractionalFrameOffset(bool),
}

impl StartTimeCommand {
    const COUNT: usize = 5;

    fn all(allow_fractional_frame_offset: bool) -> [Self; Self::COUNT] {
        [
            Self::SetTimeAtCursor,
            Self::ResetTime,
            Self::SetMeasureAtCursor,
            Self::ResetMeasure,
            Self::AllowFractionalFrameOffset(allow_fractional_frame_offset),
        ]
    }
}

impl fmt::Display for StartTimeCommand {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::SetTimeAtCursor => "Set 0:00 to current edit cursor",
            Self::ResetTime => "Reset project start time",
            Self::SetMeasureAtCursor => "Set measure 1 to nearest measure to current edit cursor",
            Self::ResetMeasure => "Reset project start measure",
            Self::AllowFractionalFrameOffset(true) => {
                "✓ Allow project start to be at a fractional frame offset"
            }
            Self::AllowFractionalFrameOffset(false) => {
                "  Allow project start to be at a fractional frame offset"
            }
        })
    }
}

pub(super) fn view(allow_fractional_frame_offset: bool) -> Element<'static, Message> {
    Element::new(StartTimeMenu {
        content: iced::widget::button("Change start time/measure...").into(),
        class: <Theme as menu::Catalog>::default(),
        commands: StartTimeCommand::all(allow_fractional_frame_offset),
    })
}

struct StartTimeMenu {
    content: Element<'static, Message>,
    class: <Theme as menu::Catalog>::Class<'static>,
    commands: [StartTimeCommand; StartTimeCommand::COUNT],
}

struct State {
    open: bool,
    hovered: Option<usize>,
    menu: menu::State,
}

fn move_selection(selected: Option<usize>, down: bool) -> usize {
    let selected = selected.unwrap_or(if down { StartTimeCommand::COUNT - 1 } else { 0 });
    if down {
        (selected + 1) % StartTimeCommand::COUNT
    } else {
        (selected + StartTimeCommand::COUNT - 1) % StartTimeCommand::COUNT
    }
}

impl Widget<Message, Theme, iced::Renderer> for StartTimeMenu {
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<State>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(State {
            open: false,
            hovered: None,
            menu: menu::State::new(),
        })
    }

    fn children(&self) -> Vec<Tree> {
        vec![Tree::new(self.content.as_widget())]
    }

    fn diff(&self, tree: &mut Tree) {
        tree.diff_children(&[self.content.as_widget()]);
    }

    fn size(&self) -> iced::Size<Length> {
        self.content.as_widget().size()
    }

    fn layout(
        &mut self,
        tree: &mut Tree,
        renderer: &iced::Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        self.content
            .as_widget_mut()
            .layout(&mut tree.children[0], renderer, limits)
    }

    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut iced::Renderer,
        theme: &Theme,
        style: &renderer::Style,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &iced::Rectangle,
    ) {
        self.content.as_widget().draw(
            &tree.children[0],
            renderer,
            theme,
            style,
            layout,
            cursor,
            viewport,
        );
    }

    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout<'_>,
        renderer: &iced::Renderer,
        operation: &mut dyn Operation,
    ) {
        self.content
            .as_widget_mut()
            .operate(&mut tree.children[0], layout, renderer, operation);
    }

    fn update(
        &mut self,
        tree: &mut Tree,
        event: &Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        renderer: &iced::Renderer,
        clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
        viewport: &iced::Rectangle,
    ) {
        let state = tree.state.downcast_mut::<State>();
        match event {
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left))
                if state.open || cursor.is_over(layout.bounds()) =>
            {
                state.open = !state.open;
                state.hovered = None;
                shell.publish(Message::ProjectStartTimeMenuChanged(state.open));
                shell.capture_event();
                shell.request_redraw();
                return;
            }
            Event::Keyboard(iced::keyboard::Event::KeyPressed {
                key: iced::keyboard::Key::Named(key),
                modifiers,
                repeat: false,
                ..
            }) if state.open && modifiers.is_empty() => {
                use iced::keyboard::key::Named;
                match key {
                    Named::ArrowDown | Named::ArrowUp => {
                        state.hovered =
                            Some(move_selection(state.hovered, *key == Named::ArrowDown));
                    }
                    Named::Enter => {
                        let selected = state
                            .hovered
                            .and_then(|index| self.commands.get(index))
                            .copied()
                            .or(Some(StartTimeCommand::SetTimeAtCursor));
                        state.open = false;
                        if let Some(command) = selected {
                            shell.publish(Message::ProjectStartTimeCommandChosen(command));
                        }
                    }
                    Named::Escape => {
                        state.open = false;
                        shell.publish(Message::ProjectStartTimeMenuChanged(false));
                    }
                    _ => return,
                }
                shell.capture_event();
                shell.request_redraw();
                return;
            }
            _ => {}
        }
        self.content.as_widget_mut().update(
            &mut tree.children[0],
            event,
            layout,
            cursor,
            renderer,
            clipboard,
            shell,
            viewport,
        );
    }

    fn mouse_interaction(
        &self,
        tree: &Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &iced::Rectangle,
        renderer: &iced::Renderer,
    ) -> mouse::Interaction {
        self.content.as_widget().mouse_interaction(
            &tree.children[0],
            layout,
            cursor,
            viewport,
            renderer,
        )
    }

    fn overlay<'a>(
        &'a mut self,
        tree: &'a mut Tree,
        layout: Layout<'_>,
        _renderer: &iced::Renderer,
        viewport: &iced::Rectangle,
        translation: iced::Vector,
    ) -> Option<iced::advanced::overlay::Element<'a, Message, Theme, iced::Renderer>> {
        let state = tree.state.downcast_mut::<State>();
        if !state.open {
            return None;
        }
        let bounds = layout.bounds();
        let menu_width = bounds.width.max(460.0);
        let origin = layout.position() + translation;
        let maximum_x = (viewport.x + viewport.width - menu_width).max(viewport.x);
        let menu_origin = iced::Point::new(origin.x.min(maximum_x).max(viewport.x), origin.y);
        Some(
            menu::Menu::new(
                &mut state.menu,
                &self.commands,
                &mut state.hovered,
                |command| {
                    state.open = false;
                    Message::ProjectStartTimeCommandChosen(command)
                },
                None,
                &self.class,
            )
            .width(menu_width)
            .padding(5)
            .overlay(menu_origin, *viewport, bounds.height, Length::Shrink),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::{StartTimeCommand, move_selection};

    #[test]
    fn start_time_menu_keyboard_moves_and_wraps_in_native_order() {
        assert_eq!(move_selection(None, true), 0);
        assert_eq!(move_selection(Some(0), true), 1);
        assert_eq!(move_selection(Some(4), true), 0);
        assert_eq!(move_selection(None, false), 4);
        assert_eq!(move_selection(Some(0), false), 4);
        let commands = StartTimeCommand::all(false);
        assert_eq!(
            commands[2].to_string(),
            "Set measure 1 to nearest measure to current edit cursor"
        );
        assert_eq!(
            commands[4].to_string(),
            "  Allow project start to be at a fractional frame offset"
        );
        assert_eq!(
            StartTimeCommand::all(true)[4].to_string(),
            "✓ Allow project start to be at a fractional frame offset"
        );
    }
}
