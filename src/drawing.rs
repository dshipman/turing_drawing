//! Simulation raster view that pins GPU memory so animated frames actually show.
//!
//! iced uploads images ≥ 2MB asynchronously. Creating a new [`Handle`] each
//! tick (as we do for the simulation) means the stock image widget often has
//! nothing ready yet and paints black. [`image_core::Renderer::load_image`]
//! uploads synchronously; holding the [`image_core::Allocation`] keeps that
//! texture alive for the draw.

use iced::advanced::image as image_core;
use iced::advanced::layout::{self, Layout};
use iced::advanced::overlay;
use iced::advanced::widget::{Operation, Tree};
use iced::advanced::{mouse, renderer, Clipboard, Shell, Widget};
use iced::widget::image::{self, FilterMethod, Handle};
use iced::{ContentFit, Element, Event, Length, Point, Rectangle, Rotation, Size, Vector};

pub fn simulation_frame(handle: Handle) -> SimulationFrame {
    SimulationFrame { handle }
}

pub struct SimulationFrame {
    handle: Handle,
}

impl<Message, Theme, Renderer> Widget<Message, Theme, Renderer> for SimulationFrame
where
    Renderer: image_core::Renderer<Handle = Handle>,
{
    fn size(&self) -> Size<Length> {
        Size {
            width: Length::Shrink,
            height: Length::Shrink,
        }
    }

    fn layout(
        &mut self,
        _tree: &mut Tree,
        renderer: &Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        image::layout(
            renderer,
            limits,
            &self.handle,
            Length::Shrink,
            Length::Shrink,
            None,
            ContentFit::Contain,
            Rotation::default(),
            true,
        )
    }

    fn draw(
        &self,
        _tree: &Tree,
        renderer: &mut Renderer,
        _theme: &Theme,
        _style: &renderer::Style,
        layout: Layout<'_>,
        _cursor: mouse::Cursor,
        _viewport: &Rectangle,
    ) {
        // Pin the allocation until this draw returns so the following
        // `draw_image` is guaranteed to have GPU memory (iced 0.14).
        let _pin = renderer.load_image(&self.handle);
        image::draw(
            renderer,
            layout,
            &self.handle,
            None,
            iced::border::Radius::default(),
            ContentFit::Contain,
            FilterMethod::Nearest,
            Rotation::default(),
            1.0,
            1.0,
        );
    }
}

impl<'a, Message, Theme, Renderer> From<SimulationFrame> for Element<'a, Message, Theme, Renderer>
where
    Renderer: image_core::Renderer<Handle = Handle> + 'a,
{
    fn from(frame: SimulationFrame) -> Self {
        Element::new(frame)
    }
}

/// Wrap `content` and emit a message on left press with local coordinates and widget size.
pub fn capture_click<'a, Message, Theme, Renderer>(
    content: impl Into<Element<'a, Message, Theme, Renderer>>,
    on_click: impl Fn(Point, Size) -> Message + 'a,
) -> Element<'a, Message, Theme, Renderer>
where
    Message: Clone + 'a,
    Theme: 'a,
    Renderer: renderer::Renderer + 'a,
{
    Element::new(ClickArea {
        content: content.into(),
        on_click: Box::new(on_click),
    })
}

struct ClickArea<'a, Message, Theme, Renderer> {
    content: Element<'a, Message, Theme, Renderer>,
    on_click: Box<dyn Fn(Point, Size) -> Message + 'a>,
}

impl<Message, Theme, Renderer> Widget<Message, Theme, Renderer>
    for ClickArea<'_, Message, Theme, Renderer>
where
    Renderer: renderer::Renderer,
    Message: Clone,
{
    fn children(&self) -> Vec<Tree> {
        vec![Tree::new(&self.content)]
    }

    fn diff(&self, tree: &mut Tree) {
        tree.diff_children(std::slice::from_ref(&self.content));
    }

    fn size(&self) -> Size<Length> {
        self.content.as_widget().size()
    }

    fn layout(
        &mut self,
        tree: &mut Tree,
        renderer: &Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        self.content
            .as_widget_mut()
            .layout(&mut tree.children[0], renderer, limits)
    }

    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout<'_>,
        renderer: &Renderer,
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
        renderer: &Renderer,
        clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
        viewport: &Rectangle,
    ) {
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

        if shell.is_event_captured() {
            return;
        }

        let pressed = matches!(
            event,
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left))
        );
        if !pressed {
            return;
        }

        let bounds = layout.bounds();
        let Some(position) = cursor.position_in(bounds) else {
            return;
        };
        shell.publish((self.on_click)(position, bounds.size()));
        shell.capture_event();
    }

    fn mouse_interaction(
        &self,
        tree: &Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
        renderer: &Renderer,
    ) -> mouse::Interaction {
        self.content.as_widget().mouse_interaction(
            &tree.children[0],
            layout,
            cursor,
            viewport,
            renderer,
        )
    }

    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut Renderer,
        theme: &Theme,
        renderer_style: &renderer::Style,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
    ) {
        self.content.as_widget().draw(
            &tree.children[0],
            renderer,
            theme,
            renderer_style,
            layout,
            cursor,
            viewport,
        );
    }

    fn overlay<'b>(
        &'b mut self,
        tree: &'b mut Tree,
        layout: Layout<'b>,
        renderer: &Renderer,
        viewport: &Rectangle,
        translation: Vector,
    ) -> Option<overlay::Element<'b, Message, Theme, Renderer>> {
        self.content.as_widget_mut().overlay(
            &mut tree.children[0],
            layout,
            renderer,
            viewport,
            translation,
        )
    }
}
