//! Animating a `height` toward the height the content takes.

use gpui::{
    AnyElement, App, AvailableSpace, Bounds, ContentMask, Element, ElementId, GlobalElementId,
    InspectorElementId, IntoElement, LayoutId, Pixels, Style, Window, px, size,
};

use crate::motion::HeightTween;

/// One element whose `height` animates with `auto` at an end of it.
///
/// `auto` is the height the content takes, and only layout knows that number.
/// GPUI lets an element lay a child out as a detached root while it requests
/// its own layout, so this measures the content there, resolves the tween
/// against the measurement, and asks for that height.
///
/// The measurement runs before this element knows its own width. A declared
/// width is what makes it exact. Without one the content measures unwrapped,
/// which reads short for text that would have wrapped.
pub(super) struct AutoHeight {
    id: u64,
    child: AnyElement,
    tween: HeightTween,
    width: Option<Pixels>,
}

impl AutoHeight {
    pub(super) fn new(
        id: u64,
        child: AnyElement,
        tween: HeightTween,
        width: Option<Pixels>,
    ) -> Self {
        Self {
            id,
            child,
            tween,
            width,
        }
    }
}

impl Element for AutoHeight {
    type RequestLayoutState = ();
    type PrepaintState = ();

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, ()) {
        let available = size(
            self.width
                .map_or(AvailableSpace::MaxContent, AvailableSpace::Definite),
            AvailableSpace::MaxContent,
        );
        let content = self.child.layout_as_root(available, window, cx);
        let height = self.tween.resolve(f64::from(f32::from(content.height)));

        let mut style = Style::default();
        style.size.height = px(height as f32).into();
        if let Some(width) = self.width {
            style.size.width = width.into();
        }
        (window.request_layout(style, [], cx), ())
    }

    fn prepaint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) {
        // The content keeps the height it measured, so the box clips while the
        // animated height is shorter than it. Taffy's `overflow` decides
        // layout, not painting, which is why this is a mask rather than a
        // style.
        window.with_content_mask(Some(ContentMask { bounds }), |window| {
            self.child.layout_as_root(
                size(
                    AvailableSpace::Definite(bounds.size.width),
                    AvailableSpace::MaxContent,
                ),
                window,
                cx,
            );
            self.child.prepaint_at(bounds.origin, window, cx);
        });
    }

    fn paint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut (),
        _prepaint: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) {
        window.with_content_mask(Some(ContentMask { bounds }), |window| {
            self.child.paint(window, cx);
        });
        // The child painted its own tracker at the height it measured. The box
        // on screen is this one, so it records last and wins.
        crate::automation::record_bounds(self.id, bounds);
    }
}

impl IntoElement for AutoHeight {
    type Element = Self;

    fn into_element(self) -> Self {
        self
    }
}
