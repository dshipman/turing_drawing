//! Simulation raster view that pins GPU memory so animated frames actually show.
//!
//! iced uploads images ≥ 2MB asynchronously. Creating a new [`Handle`] each
//! tick (as we do for the simulation) means the stock image widget often has
//! nothing ready yet and paints black. [`image_core::Renderer::load_image`]
//! uploads synchronously; holding the [`image_core::Allocation`] keeps that
//! texture alive for the draw.

use iced::advanced::image as image_core;
use iced::advanced::layout::{self, Layout};
use iced::advanced::widget::Tree;
use iced::advanced::{mouse, renderer, Widget};
use iced::widget::image::{self, FilterMethod, Handle};
use iced::{ContentFit, Element, Length, Rectangle, Rotation, Size};

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
