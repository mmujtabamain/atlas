//! [`Scaled`]: an element that draws its child scaled about a point, the
//! way a CSS `transform: scale()` does, without laying it out any differently.
//!
//! The child is laid out and prepainted at its normal size in its normal
//! place; at paint time its whole painted result — boxes, borders, shadows,
//! text, images — is scaled through [`Window::with_scale`], the engine's
//! paint-time transform. Hitboxes keep their laid-out places, so while the
//! child is drawn at any size but its own the element takes its clicks and
//! its wheel before they reach it: a scaled pane is on show, not in use.
//! Drags and drops pass, because they are what a scaled pane is there for.
//!
//! The workspace skin does not apply it for now: while a tab is held the
//! cards shrink through the layout alone, so their contents keep their size.
//! It stays here, with the engine addition it rests on, for when it is
//! wanted again.

use std::cell::Cell;
use std::rc::Rc;

use gpui_kit::*;

/// The point a [`Scaled`] child shrinks towards.
#[derive(Clone, Debug)]
pub enum ScaleOrigin {
    /// The centre of the child's own bounds.
    Center,
    /// The centre of another rectangle, in window coordinates, read when the
    /// child is painted — for a child that is one part of a larger whole
    /// scaled as one (a card's tab bar and its content share the card's).
    CenterOf(Rc<Cell<Bounds<Pixels>>>),
}

/// The scale at which the child counts as shown at its own size.
const AT_REST: f32 = 0.999;

/// Draws its child scaled about an origin; see the module docs.
pub struct Scaled {
    child: AnyElement,
    scale: f32,
    origin: ScaleOrigin,
}

impl Scaled {
    /// `child` drawn at `scale` times its size about `origin`. A scale of 1
    /// draws it as it is and leaves its input alone.
    pub fn new(scale: f32, origin: ScaleOrigin, child: impl IntoElement) -> Self {
        Scaled { child: child.into_any_element(), scale, origin }
    }

    fn is_scaled(&self) -> bool {
        self.scale < AT_REST || self.scale > 1. / AT_REST
    }
}

impl IntoElement for Scaled {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for Scaled {
    type RequestLayoutState = ();
    type PrepaintState = ();

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        None
    }

    fn request_layout(&mut self, _: Option<&GlobalElementId>, _: Option<&InspectorElementId>, window: &mut Window, cx: &mut App) -> (LayoutId, ()) {
        (self.child.request_layout(window, cx), ())
    }

    fn prepaint(&mut self, _: Option<&GlobalElementId>, _: Option<&InspectorElementId>, _: Bounds<Pixels>, _: &mut (), window: &mut Window, cx: &mut App) {
        self.child.prepaint(window, cx);
    }

    fn paint(&mut self, _: Option<&GlobalElementId>, _: Option<&InspectorElementId>, bounds: Bounds<Pixels>, _: &mut (), _: &mut (), window: &mut Window, cx: &mut App) {
        if !self.is_scaled() {
            self.child.paint(window, cx);
            return;
        }
        // Clicks and the wheel stop here, before the child sees them; a
        // release with a drag in flight is a drop, and goes through.
        window.on_mouse_event(move |event: &MouseDownEvent, phase, _, cx| {
            if phase == DispatchPhase::Capture && bounds.contains(&event.position) {
                cx.stop_propagation();
            }
        });
        window.on_mouse_event(move |event: &ScrollWheelEvent, phase, _, cx| {
            if phase == DispatchPhase::Capture && bounds.contains(&event.position) {
                cx.stop_propagation();
            }
        });
        let origin = match &self.origin {
            ScaleOrigin::Center => bounds.center(),
            ScaleOrigin::CenterOf(whole) => {
                let whole = whole.get();
                if whole.size.width > px(0.) { whole.center() } else { bounds.center() }
            }
        };
        window.with_scale(self.scale, origin, |window| self.child.paint(window, cx));
    }
}
