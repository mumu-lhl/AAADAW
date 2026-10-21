use super::super::Message;
use aaadaw_core::FrameRate;
use iced::advanced::widget::{Operation, Tree, tree};
use iced::advanced::{Clipboard, Layout, Shell, Widget, layout, mouse, renderer};
use iced::widget::overlay::menu;
use iced::{Element, Event, Length, Theme};

pub(super) fn view(rate: FrameRate) -> Element<'static, Message> {
    Element::new(FrameRateMenu {
        rate,
        content: iced::widget::pick_list(
            FrameRate::ALL,
            Some(rate),
            Message::ProjectFrameRateChanged,
        )
        .into(),
        class: <Theme as menu::Catalog>::default(),
    })
}

struct FrameRateMenu {
    rate: FrameRate,
    content: Element<'static, Message>,
    class: <Theme as menu::Catalog>::Class<'static>,
}

struct State {
    open: bool,
    hovered: Option<usize>,
    menu: menu::State,
}

fn move_selection(selected: Option<usize>, down: bool) -> usize {
    let selected = selected.unwrap_or(0);
    if down {
        (selected + 1) % FrameRate::ALL.len()
    } else {
        (selected + FrameRate::ALL.len() - 1) % FrameRate::ALL.len()
    }
}

impl Widget<Message, Theme, iced::Renderer> for FrameRateMenu {
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
                if state.open {
                    state.hovered = FrameRate::ALL.iter().position(|rate| *rate == self.rate);
                }
                shell.publish(Message::ProjectFrameRateMenuChanged(state.open));
                shell.capture_event();
                shell.request_redraw();
                return;
            }
            Event::Keyboard(iced::keyboard::Event::KeyPressed {
                key: iced::keyboard::Key::Named(key),
                modifiers,
                ..
            }) if state.open && modifiers.is_empty() => {
                use iced::keyboard::key::Named;
                match key {
                    Named::ArrowDown | Named::ArrowUp => {
                        state.hovered =
                            Some(move_selection(state.hovered, *key == Named::ArrowDown));
                    }
                    Named::Enter => {
                        state.open = false;
                        if let Some(rate) =
                            state.hovered.and_then(|index| FrameRate::ALL.get(index))
                        {
                            shell.publish(Message::ProjectFrameRateChanged(*rate));
                        }
                        shell.publish(Message::ProjectFrameRateMenuChanged(false));
                    }
                    Named::Escape => {
                        state.open = false;
                        shell.publish(Message::ProjectFrameRateMenuChanged(false));
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
        Some(
            menu::Menu::new(
                &mut state.menu,
                &FrameRate::ALL,
                &mut state.hovered,
                |rate| {
                    state.open = false;
                    Message::ProjectFrameRateChanged(rate)
                },
                None,
                &self.class,
            )
            .width(bounds.width)
            .padding(5)
            .overlay(
                layout.position() + translation,
                *viewport,
                bounds.height,
                Length::Shrink,
            ),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn frame_menu_direction_keys_follow_measured_preset_order_and_wrap() {
        let start = FrameRate::ALL
            .iter()
            .position(|rate| *rate == FrameRate::Fps30)
            .unwrap();
        assert_eq!(
            FrameRate::ALL[move_selection(Some(start), true)],
            FrameRate::Fps48
        );
        assert_eq!(
            FrameRate::ALL[move_selection(Some(start), false)],
            FrameRate::Fps2997
        );
        assert_eq!(move_selection(Some(0), false), 9);
        assert_eq!(move_selection(Some(9), true), 0);
    }
}
