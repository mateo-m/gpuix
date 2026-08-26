//! One place that turns a `StyleDesc` into a GPUI `StyleRefinement`.
//!
//! GPUI is immediate mode. It rebuilds the element tree every frame. Before
//! this module the renderer ran 52 `if let Some` branches for every element on
//! every frame, and it ran them again for styles that had not changed since the
//! last mutation from React.
//!
//! `StyleRefinement` is the type the whole GPUI style API already speaks.
//! `Styled::style()` returns `&mut StyleRefinement`, and `hover`, `active`,
//! `group_hover` and `group_active` all take
//! `impl FnOnce(StyleRefinement) -> StyleRefinement`. `Refineable` also merges a
//! refinement into another refinement, so one resolved value covers the base
//! style and every variant. That makes the resolved refinement a cache the
//! renderer can hold and reuse until the style changes.

use std::sync::atomic::{AtomicU64, Ordering};

use gpui::{Refineable, StyleRefinement};

use crate::inheritance::Inherited;
use crate::style::vars::Scope;
use crate::style::StyleDesc;

/// How many times `resolve` ran since the last reset.
///
/// The performance tests assert on this counter instead of on wall-clock time.
/// A steady-state frame must add zero. One `setStyle` must add one. A wall-clock
/// budget flakes on a loaded machine and then someone mutes it. A counter does
/// not flake, and it fails loudly when a cache stops working.
static RESOLUTIONS: AtomicU64 = AtomicU64::new(0);

/// Read the resolve counter.
pub(crate) fn resolutions() -> u64 {
    RESOLUTIONS.load(Ordering::Relaxed)
}

/// Set the resolve counter back to zero.
pub(crate) fn reset_resolutions() {
    RESOLUTIONS.store(0, Ordering::Relaxed);
}

/// One state pseudo-class, which is one kind of condition.
///
/// GPUI evaluates these itself at paint, with no re-render and no second
/// resolve, so a pointer moving over an element costs nothing in this crate.
/// That is why states live beside the resolved style rather than inside it.
///
/// Conditions are an open set. Adding `:focus` is one variant here and one arm
/// at the paint site, not a new field on every resolution in the tree.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum State {
    Hover,
    Active,
}

/// A `StyleDesc` with every value turned into a GPUI value.
///
/// The renderer stores this on the retained element and drops it when the style
/// changes. Applying it to an element costs one `refine` call per state.
#[derive(Debug, Clone)]
pub(crate) struct Resolved {
    pub base: StyleRefinement,
    /// The states this style declares, in the order `StyleDesc::states` lists
    /// them.
    ///
    /// Almost every element declares none. An empty `Vec` allocates nothing,
    /// where the two inline `Option<StyleRefinement>` fields this replaced
    /// carried the full size of a refinement each whether or not anything used
    /// them.
    pub states: Vec<(State, StyleRefinement)>,
    /// The cascade this resolution read, or `None` when it read nothing
    /// inherited.
    ///
    /// A style with no `var()` and no `currentColor` computes the same value
    /// under every cascade, so `None` means the cached resolution stays valid
    /// however the cascade changes above it. That is almost every element, and
    /// it keeps the cost of custom properties on the elements that use them.
    pub cascade: Option<Inherited>,
}

impl Resolved {
    /// Resolve a style and every state it declares against a cascade.
    pub fn build(style: &StyleDesc, cascade: &Inherited) -> Self {
        let scope = cascade.scope();
        let base = resolve(style, &scope);
        let states = style
            .states()
            .map(|(state, declared)| (state, resolve(declared, &scope)))
            .collect();
        Self {
            base,
            states,
            cascade: scope.used_a_variable().then(|| cascade.clone()),
        }
    }

    /// The refinement for one state, or `None` when the style does not declare
    /// it.
    pub fn state(&self, state: State) -> Option<&StyleRefinement> {
        self.states
            .iter()
            .find(|(declared, _)| *declared == state)
            .map(|(_, refinement)| refinement)
    }

    /// Whether this resolution still holds under `cascade`.
    pub fn valid_under(&self, cascade: &Inherited) -> bool {
        match &self.cascade {
            None => true,
            Some(read) => read.same(cascade),
        }
    }
}

/// Turn one `StyleDesc` into a `StyleRefinement`.
///
/// `apply_styles` is generic over `E: Styled`, so the compiler proves it only
/// calls style setters. That makes this wrapper the same work the renderer did
/// before, moved off the frame path.
pub(crate) fn resolve(style: &StyleDesc, scope: &Scope) -> StyleRefinement {
    RESOLUTIONS.fetch_add(1, Ordering::Relaxed);
    apply_styles(StyleRefinement::default(), style, scope)
}

/// Merge a resolved refinement into any styled element.
///
/// This is the whole per-frame cost of styling one element.
pub(crate) fn apply_resolved<E: gpui::Styled>(mut el: E, resolved: &StyleRefinement) -> E {
    el.style().refine(resolved);
    el
}

/// Apply a motion frame on top of a resolved style.
///
/// Motion drives eight numbers, and none of them reads a variable,
/// `currentColor` or the font size. Every one of them lands on the element
/// here, so an animated element keeps the cached resolution of everything it
/// declared. Folding the numbers into a `StyleDesc` and resolving that instead
/// reparsed every declaration the element made, on every frame, to change one
/// value.
pub(crate) fn apply_motion<E: gpui::Styled>(
    mut el: E,
    frame: &crate::motion::MotionFrame,
    declared: Option<&StyleDesc>,
) -> E {
    let motion = frame.style;
    if let Some(width) = motion.width {
        el = el.w(gpui::px(width as f32));
    }
    match motion.height.map(crate::motion::MotionHeight::length) {
        Some(Some(height)) => el = el.h(gpui::px(height as f32)),
        // A height that still needs the content has no number yet.
        // `auto_height::wrap` measures this element to find one, so this
        // element must not declare a height of its own.
        Some(None) => el.style().size.height = Some(gpui::Length::Auto),
        None => {}
    }
    if let Some(top) = motion.top {
        el = el.top(gpui::px(top as f32));
    }
    if let Some(right) = motion.right {
        el = el.right(gpui::px(right as f32));
    }
    if let Some(bottom) = motion.bottom {
        el = el.bottom(gpui::px(bottom as f32));
    }
    if let Some(left) = motion.left {
        el = el.left(gpui::px(left as f32));
    }
    if let Some(radius) = motion.border_radius {
        // A declared corner longhand beats the shorthand, which is the order
        // `apply_styles` reads the two in, so motion leaves that corner alone.
        let radius = gpui::px(radius as f32);
        let free = |declares: fn(&StyleDesc) -> bool| !declared.is_some_and(declares);
        if free(|style| style.border_top_left_radius.is_some()) {
            el = el.rounded_tl(radius);
        }
        if free(|style| style.border_top_right_radius.is_some()) {
            el = el.rounded_tr(radius);
        }
        if free(|style| style.border_bottom_left_radius.is_some()) {
            el = el.rounded_bl(radius);
        }
        if free(|style| style.border_bottom_right_radius.is_some()) {
            el = el.rounded_br(radius);
        }
    }
    if let Some(shape) = motion.corner_shape {
        // Same rule as the radius: a property narrower than `cornerShape`
        // keeps its corner. `corner` and `cornerShape` are what motion drives.
        let narrow = declared.map(|style| {
            let wide = StyleDesc {
                corner: None,
                corner_shape: None,
                ..style.clone()
            };
            super::corners::resolve(&wide).shapes
        });
        let shape = gpui::CornerShape(shape.0 as f32);
        let free = |pick: fn(&gpui::Corners<Option<f32>>) -> Option<f32>| {
            narrow.as_ref().and_then(pick).is_none()
        };
        if free(|c| c.top_left) {
            el = el.corner_shape_tl(shape);
        }
        if free(|c| c.top_right) {
            el = el.corner_shape_tr(shape);
        }
        if free(|c| c.bottom_left) {
            el = el.corner_shape_bl(shape);
        }
        if free(|c| c.bottom_right) {
            el = el.corner_shape_br(shape);
        }
    }
    if let Some(opacity) = motion.opacity {
        el = el.opacity(opacity as f32);
    }
    el
}

// ── Style application ────────────────────────────────────────────────

/// The six sizing properties, each read the same way and each landing in its
/// own slot. `Auto` is what all six already default to, so writing it changes
/// nothing.
fn apply_sizes<E: gpui::Styled>(mut el: E, style: &StyleDesc, scope: &Scope) -> E {
    let sizes = el.style();
    for (declared, slot) in [
        (&style.width, &mut sizes.size.width),
        (&style.height, &mut sizes.size.height),
        (&style.min_width, &mut sizes.min_size.width),
        (&style.min_height, &mut sizes.min_size.height),
        (&style.max_width, &mut sizes.max_size.width),
        (&style.max_height, &mut sizes.max_size.height),
    ] {
        if let Some(value) = scope.dimension(declared) {
            *slot = Some(dimension(value));
        }
    }
    el
}

/// The GPUI length a resolved sizing value means.
fn dimension(value: crate::style::DimensionValue) -> gpui::Length {
    match value {
        crate::style::DimensionValue::Pixels(pixels) => gpui::px(pixels as f32).into(),
        // A hair under the whole is a rounded 100%, and a whole is what
        // `w_full` writes.
        crate::style::DimensionValue::Percentage(share) if share >= 0.999 => {
            gpui::relative(1.0).into()
        }
        crate::style::DimensionValue::Percentage(share) => gpui::relative(share as f32).into(),
        crate::style::DimensionValue::Auto => gpui::Length::Auto,
    }
}

/// The one fill an element paints, or none.
///
/// GPUI paints one fill per box, so an image wins over a colour outright. A
/// browser would paint the image over the colour, which only differs when
/// the image has transparent parts. `background` is the shorthand, so it
/// loses to both longhands.
fn background_fill(style: &StyleDesc, scope: &Scope) -> Option<gpuix_css::background::Fill> {
    let image = style
        .background_image
        .as_deref()
        .and_then(|text| scope.fill(text));
    if image.is_some() {
        return image;
    }
    style
        .background_color
        .as_deref()
        .or(style.background.as_deref())
        .and_then(|text| scope.fill(text))
}

pub(crate) fn apply_styles<E: gpui::Styled>(mut el: E, style: &StyleDesc, scope: &Scope) -> E {
    // `visibility` reached StyleDesc but nothing read it, so `hideInstance`
    // hid nothing. GPUI's Visibility::Hidden has the CSS meaning: skip the
    // paint, keep the layout box.
    match style.visibility.as_deref() {
        Some("hidden") => el.style().visibility = Some(gpui::Visibility::Hidden),
        Some("visible") => el.style().visibility = Some(gpui::Visibility::Visible),
        _ => {}
    }
    match style.display.as_deref() {
        Some("flex") => el = el.flex(),
        Some("grid") => el = el.grid(),
        _ => {}
    }
    if let Some(cols) = scope.number(&style.grid_template_columns) {
        let count = cols.round().clamp(1.0, 64.0) as u16;
        el = match style.grid_column_min.as_deref() {
            Some("min-content") => el.grid_cols_min_content(count),
            Some("max-content") => el.grid_cols_max_content(count),
            _ => el.grid_cols(count),
        };
    }
    if let Some(rows) = scope.number(&style.grid_template_rows) {
        let count = rows.round().clamp(1.0, 64.0) as u16;
        el = match style.grid_row_min.as_deref() {
            Some("min-content") => el.grid_rows_min_content(count),
            Some("max-content") => el.grid_rows_max_content(count),
            _ => el.grid_rows(count),
        };
    }
    if style.flex_direction.as_deref() == Some("column") {
        el = el.flex_col();
    }
    if style.flex_direction.as_deref() == Some("row") {
        el = el.flex_row();
    }
    match style.flex_wrap.as_deref() {
        Some("wrap") => el = el.flex_wrap(),
        Some("wrap-reverse") => el = el.flex_wrap_reverse(),
        Some("nowrap") => el = el.flex_nowrap(),
        _ => {}
    }
    if let Some(grow) = scope.number(&style.flex_grow) {
        el.style().flex_grow = Some(grow as f32);
    }
    if let Some(shrink) = scope.number(&style.flex_shrink) {
        el.style().flex_shrink = Some(shrink as f32);
    }
    if let Some(basis) = scope.number(&style.flex_basis) {
        el = el.flex_basis(gpui::px(basis as f32));
    }
    match style.align_items.as_deref() {
        Some("center") => el = el.items_center(),
        Some("start") | Some("flex-start") => el = el.items_start(),
        Some("end") | Some("flex-end") => el = el.items_end(),
        _ => {}
    }
    match style.align_content.as_deref() {
        Some("center") => el = el.content_center(),
        Some("start") | Some("flex-start") => el = el.content_start(),
        Some("end") | Some("flex-end") => el = el.content_end(),
        Some("between") | Some("space-between") => el = el.content_between(),
        Some("around") | Some("space-around") => el = el.content_around(),
        Some("evenly") | Some("space-evenly") => el = el.content_evenly(),
        Some("stretch") => el = el.content_stretch(),
        Some("normal") => el = el.content_normal(),
        _ => {}
    }
    match style.justify_content.as_deref() {
        Some("center") => el = el.justify_center(),
        Some("start") | Some("flex-start") => el = el.justify_start(),
        Some("end") | Some("flex-end") => el = el.justify_end(),
        Some("between") | Some("space-between") => el = el.justify_between(),
        Some("around") | Some("space-around") => el = el.justify_around(),
        _ => {}
    }
    match style.align_self.as_deref() {
        Some("center") => {
            el.style().align_self = Some(gpui::AlignItems::Center);
        }
        Some("start") | Some("flex-start") => {
            el.style().align_self = Some(gpui::AlignItems::FlexStart);
        }
        Some("end") | Some("flex-end") => {
            el.style().align_self = Some(gpui::AlignItems::FlexEnd);
        }
        Some("stretch") => {
            el.style().align_self = Some(gpui::AlignItems::Stretch);
        }
        Some("baseline") => {
            el.style().align_self = Some(gpui::AlignItems::Baseline);
        }
        _ => {}
    }
    if let Some(gap) = scope.number(&style.gap) {
        el = el.gap(gpui::px(gap as f32));
    }
    // Per-axis gaps were in the style type and implemented nowhere. They come
    // after `gap` so the axis value wins, matching CSS shorthand order.
    if let Some(gap) = scope.number(&style.row_gap) {
        el = el.gap_y(gpui::px(gap as f32));
    }
    if let Some(gap) = scope.number(&style.column_gap) {
        el = el.gap_x(gpui::px(gap as f32));
    }
    el = apply_sizes(el, style, scope);
    if let Some(p) = scope.number(&style.padding) {
        el = el.p(gpui::px(p as f32));
    }
    if let Some(pt) = scope.number(&style.padding_top) {
        el = el.pt(gpui::px(pt as f32));
    }
    if let Some(pr) = scope.number(&style.padding_right) {
        el = el.pr(gpui::px(pr as f32));
    }
    if let Some(pb) = scope.number(&style.padding_bottom) {
        el = el.pb(gpui::px(pb as f32));
    }
    if let Some(pl) = scope.number(&style.padding_left) {
        el = el.pl(gpui::px(pl as f32));
    }
    if let Some(m) = scope.number(&style.margin) {
        el = el.m(gpui::px(m as f32));
    }
    if let Some(mt) = scope.number(&style.margin_top) {
        el = el.mt(gpui::px(mt as f32));
    }
    if let Some(mr) = scope.number(&style.margin_right) {
        el = el.mr(gpui::px(mr as f32));
    }
    if let Some(mb) = scope.number(&style.margin_bottom) {
        el = el.mb(gpui::px(mb as f32));
    }
    if let Some(ml) = scope.number(&style.margin_left) {
        el = el.ml(gpui::px(ml as f32));
    }
    // Taffy has no viewport-fixed position, and GPUI has no scrolling document,
    // so "fixed" lays out exactly like "absolute". `should_occlude` already
    // treats the two the same. Without this arm a "fixed" box stayed in flow.
    match style.position.as_deref() {
        Some("absolute") | Some("fixed") => el = el.absolute(),
        Some("relative") => el = el.relative(),
        _ => {}
    }
    if let Some(top) = scope.number(&style.top) {
        el = el.top(gpui::px(top as f32));
    }
    if let Some(right) = scope.number(&style.right) {
        el = el.right(gpui::px(right as f32));
    }
    if let Some(bottom) = scope.number(&style.bottom) {
        el = el.bottom(gpui::px(bottom as f32));
    }
    if let Some(left) = scope.number(&style.left) {
        el = el.left(gpui::px(left as f32));
    }
    if let Some(fill) = background_fill(style, scope) {
        el = el.bg(crate::color::to_background(&fill));
    }
    if let Some(color) = style.color.as_deref().and_then(|c| scope.color(c)) {
        el = el.text_color(crate::color::to_hsla(color));
    }
    if let Some(size) = scope.number(&style.font_size) {
        el = el.text_size(gpui::px(size as f32));
    }
    if let Some(ref family) = style.font_family {
        el = el.font_family(family.clone());
    }
    if let Some(ref weight) = style.font_weight {
        el = el.font_weight(parse_font_weight(weight));
    }
    // `textAlign` was in the style type but implemented nowhere.
    match style.text_align.as_deref() {
        Some("center") => el = el.text_center(),
        Some("right") => el = el.text_right(),
        Some("left") | Some("start") => el = el.text_left(),
        _ => {}
    }
    match style.white_space.as_deref() {
        Some("nowrap") => el = el.whitespace_nowrap(),
        Some("normal") => el = el.whitespace_normal(),
        _ => {}
    }
    match style.text_overflow.as_deref() {
        Some("ellipsis") => el = el.text_ellipsis(),
        Some("ellipsis-start") => el = el.text_ellipsis_start(),
        _ => {}
    }
    if let Some(clamp) = scope.number(&style.line_clamp) {
        if clamp >= 1.0 {
            el = el.line_clamp(clamp as usize);
        }
    }
    // A JS number is pixels, so `lineHeight: 20` is 20px as in the upstream
    // API. A string follows CSS: a bare number is a multiple of the font
    // size, and a length is that length.
    if let Some(crate::style::Numeric::Number(pixels)) = style.line_height {
        if pixels > 0.0 {
            el = el.line_height(gpui::px(pixels as f32));
        }
    } else if let Some(line_height) = scope.length(&style.line_height) {
        match line_height {
            gpuix_css::length::Length::Number(multiple)
            | gpuix_css::length::Length::Fraction(multiple)
                if multiple > 0.0 =>
            {
                el = el.line_height(gpui::relative(multiple));
            }
            gpuix_css::length::Length::Pixels(pixels) if pixels > 0.0 => {
                el = el.line_height(gpui::px(pixels));
            }
            _ => {}
        }
    }
    let corners = super::corners::resolve(style);
    if let Some(radius) = scope.number(&corners.radii.top_left) {
        el = el.rounded_tl(gpui::px(radius as f32));
    }
    if let Some(radius) = scope.number(&corners.radii.top_right) {
        el = el.rounded_tr(gpui::px(radius as f32));
    }
    if let Some(radius) = scope.number(&corners.radii.bottom_left) {
        el = el.rounded_bl(gpui::px(radius as f32));
    }
    if let Some(radius) = scope.number(&corners.radii.bottom_right) {
        el = el.rounded_br(gpui::px(radius as f32));
    }
    if let Some(shape) = corners.shapes.top_left {
        el = el.corner_shape_tl(gpui::CornerShape(shape));
    }
    if let Some(shape) = corners.shapes.top_right {
        el = el.corner_shape_tr(gpui::CornerShape(shape));
    }
    if let Some(shape) = corners.shapes.bottom_left {
        el = el.corner_shape_bl(gpui::CornerShape(shape));
    }
    if let Some(shape) = corners.shapes.bottom_right {
        el = el.corner_shape_br(gpui::CornerShape(shape));
    }
    // `borderWidth: 0` must clear a border, not be ignored: an element that
    // draws its own border needs a way for the caller to remove it.
    if let Some(width) = scope.number(&style.border_width) {
        el = el.border(gpui::px(width.max(0.0) as f32));
    }
    if let Some(width) = scope.number(&style.border_top_width) {
        el = el.border_t(gpui::px(width.max(0.0) as f32));
    }
    if let Some(width) = scope.number(&style.border_right_width) {
        el = el.border_r(gpui::px(width.max(0.0) as f32));
    }
    if let Some(width) = scope.number(&style.border_bottom_width) {
        el = el.border_b(gpui::px(width.max(0.0) as f32));
    }
    if let Some(width) = scope.number(&style.border_left_width) {
        el = el.border_l(gpui::px(width.max(0.0) as f32));
    }
    if let Some(color) = style.border_color.as_deref().and_then(|c| scope.color(c)) {
        el = el.border_color(crate::color::to_hsla(color));
    }
    if let Some(ref shadow) = style.box_shadow {
        if let Some(color) = scope.color(&shadow.color) {
            let shadow = gpui::BoxShadow::new(
                gpui::px(shadow.offset_x as f32),
                gpui::px(shadow.offset_y as f32),
                crate::color::to_hsla(color),
            )
            .blur_radius(gpui::px(shadow.blur_radius.max(0.0) as f32))
            .spread_radius(gpui::px(shadow.spread_radius as f32));
            el = el.shadow(vec![shadow]);
        }
    }
    if let Some(opacity) = scope.number(&style.opacity) {
        el = el.opacity(opacity as f32);
    }
    if let Some(cursor) = style.cursor.as_deref().and_then(cursor_style) {
        el = el.cursor(cursor);
    }
    // Overflow: hidden is on the Styled trait, so we handle it here.
    // overflow: "scroll" requires StatefulInteractiveElement — handled in build_div().
    // CSS precedence: axis-specific (overflowX/Y) overrides the shorthand (overflow).
    {
        let resolved_x = style.overflow_x.as_deref().or(style.overflow.as_deref());
        let resolved_y = style.overflow_y.as_deref().or(style.overflow.as_deref());
        // Only apply hidden here — scroll is handled in build_div.
        if resolved_x == Some("hidden") && resolved_y == Some("hidden") {
            el = el.overflow_hidden();
        } else if resolved_x == Some("hidden") {
            el = el.overflow_x_hidden();
        } else if resolved_y == Some("hidden") {
            el = el.overflow_y_hidden();
        }
    }

    el
}

/// Parse a CSS font-weight value (string or number) into a GPUI FontWeight.
/// Accepts named keywords ("bold", "semibold"), numeric strings ("700"),
/// and raw numbers (700). Falls back to 400 (normal) for unrecognized values.
pub(crate) fn parse_font_weight(value: &crate::style::FontWeightValue) -> gpui::FontWeight {
    match value {
        crate::style::FontWeightValue::Num(n) => gpui::FontWeight((*n as f32).clamp(1.0, 1000.0)),
        crate::style::FontWeightValue::Str(s) => {
            let lower = s.trim().to_ascii_lowercase();
            match lower.as_str() {
                "100" | "thin" => gpui::FontWeight(100.0),
                "200" | "extralight" | "extra-light" => gpui::FontWeight(200.0),
                "300" | "light" => gpui::FontWeight(300.0),
                "400" | "normal" => gpui::FontWeight(400.0),
                "500" | "medium" => gpui::FontWeight(500.0),
                "600" | "semibold" | "semi-bold" => gpui::FontWeight(600.0),
                "700" | "bold" => gpui::FontWeight(700.0),
                "800" | "extrabold" | "extra-bold" => gpui::FontWeight(800.0),
                "900" | "black" => gpui::FontWeight(900.0),
                _ => lower
                    .parse::<f32>()
                    .map(|n| gpui::FontWeight(n.clamp(1.0, 1000.0)))
                    .unwrap_or(gpui::FontWeight(400.0)),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn styled(color: &str) -> Box<StyleDesc> {
        Box::new(StyleDesc {
            background_color: Some(color.to_string()),
            ..Default::default()
        })
    }

    /// A cascade with nothing declared above it.
    fn no_variables() -> Inherited {
        let theme = crate::theme::Theme::default();
        Inherited::root(crate::color::from_gpui(theme.accent), theme.dark, 16.0)
    }

    /// A cascade with `pairs` declared one level down.
    fn variables(pairs: &[(&str, &str)]) -> Inherited {
        let custom = pairs
            .iter()
            .map(|(name, value)| (name.to_string(), serde_json::json!(value)))
            .collect();
        let style = StyleDesc {
            custom,
            ..Default::default()
        };
        no_variables().descend(Some(&style))
    }

    fn background_of(style: &StyleDesc, cascade: &Inherited) -> Option<gpui::Fill> {
        Resolved::build(style, cascade).base.background
    }

    fn fill(color: &str) -> Option<gpui::Fill> {
        Some(crate::color::parse_color_rgba(color).unwrap().into())
    }

    #[test]
    fn resolves_the_base_style_and_each_state() {
        let style = StyleDesc {
            background_color: Some("#111111".to_string()),
            hover: Some(styled("#ff0000")),
            active: Some(styled("#00ff00")),
            ..Default::default()
        };
        let cascade = no_variables();
        let resolved = Resolved::build(&style, &cascade);
        let plain = cascade.scope();
        assert_eq!(
            resolved.state(State::Hover),
            Some(&resolve(&styled("#ff0000"), &plain))
        );
        assert_eq!(
            resolved.state(State::Active),
            Some(&resolve(&styled("#00ff00"), &plain))
        );
        // The list keeps the order `StyleDesc::states` declares, which is the
        // order the paint dispatcher walks.
        assert_eq!(
            resolved.states.iter().map(|(s, _)| *s).collect::<Vec<_>>(),
            vec![State::Hover, State::Active]
        );
    }

    #[test]
    fn a_gradient_image_wins_over_the_colour() {
        let style = StyleDesc {
            background_color: Some("#111111".to_string()),
            background_image: Some("linear-gradient(to right, red, blue)".to_string()),
            ..Default::default()
        };
        let fill = background_of(&style, &no_variables()).expect("a fill");
        let background = fill.color().expect("a background");
        assert!(background.as_solid().is_none(), "should be a gradient: {background:?}");

        // `none` steps aside for the colour underneath.
        let style = StyleDesc {
            background_image: Some("none".to_string()),
            ..style
        };
        let fill = background_of(&style, &no_variables()).expect("a fill");
        assert!(fill.color().and_then(|b| b.as_solid()).is_some());

        // The shorthand takes a gradient too.
        let shorthand = StyleDesc {
            background: Some("linear-gradient(red, blue)".to_string()),
            ..Default::default()
        };
        let fill = background_of(&shorthand, &no_variables()).expect("a fill");
        assert!(fill.color().and_then(|b| b.as_solid()).is_none());
    }

    #[test]
    fn a_style_with_no_states_resolves_to_none() {
        let resolved = Resolved::build(&styled("#111111"), &no_variables());
        assert!(resolved.states.is_empty());
        assert!(resolved.state(State::Hover).is_none());
        assert!(resolved.state(State::Active).is_none());
    }

    #[test]
    fn an_unknown_style_field_does_not_fail_the_whole_style() {
        // A newer client must lose one declaration, not its element.
        let json = r##"{ "backgroundColor": "#111111", "someFutureThing": 4 }"##;
        let style: StyleDesc = serde_json::from_str(json).expect("style should still parse");
        assert_eq!(style.background_color.as_deref(), Some("#111111"));
    }

    #[test]
    fn a_variable_reaches_a_colour() {
        let scope = variables(&[("--brand", "#ff0000")]);
        assert_eq!(
            background_of(&styled("var(--brand)"), &scope),
            fill("#ff0000")
        );
    }

    #[test]
    fn a_variable_reaches_a_state_colour() {
        let style = StyleDesc {
            hover: Some(styled("var(--brand)")),
            ..Default::default()
        };
        let scope = variables(&[("--brand", "#ff0000")]);
        let resolved = Resolved::build(&style, &scope);
        assert_eq!(
            resolved.state(State::Hover).and_then(|h| h.background.clone()),
            fill("#ff0000")
        );
    }

    #[test]
    fn a_missing_variable_leaves_the_colour_unset() {
        // CSS calls this invalid at computed-value time. The property takes the
        // value it would have had, which here is no background at all.
        assert_eq!(background_of(&styled("var(--nope)"), &no_variables()), None);
    }

    #[test]
    fn a_fallback_paints_when_the_variable_is_missing() {
        assert_eq!(
            background_of(&styled("var(--nope, #00ff00)"), &no_variables()),
            fill("#00ff00")
        );
    }

    #[test]
    fn a_style_that_reads_nothing_holds_under_every_cascade() {
        // This is what keeps custom properties off the cost of every other
        // element. A resolution that read nothing is never invalidated.
        let resolved = Resolved::build(&styled("#111111"), &no_variables());
        assert!(resolved.cascade.is_none());
        assert!(resolved.valid_under(&variables(&[("--brand", "#ff0000")])));
    }

    #[test]
    fn a_style_that_read_a_variable_only_holds_under_that_cascade() {
        let cascade = variables(&[("--brand", "#ff0000")]);
        let resolved = Resolved::build(&styled("var(--brand)"), &cascade);
        assert!(resolved.valid_under(&cascade));
        assert!(!resolved.valid_under(&variables(&[("--brand", "#ff0000")])));
    }

    #[test]
    fn a_var_that_falls_back_still_counts_as_reading_the_cascade() {
        // The fallback won because nothing declared the variable. A different
        // cascade could declare one, so the resolution has to be bound to it.
        let resolved = Resolved::build(&styled("var(--brand, #00ff00)"), &no_variables());
        assert!(resolved.cascade.is_some());
    }

    #[test]
    fn current_color_reads_the_inherited_colour() {
        let cascade = no_variables().descend(Some(&StyleDesc {
            color: Some("#ff0000".to_string()),
            ..Default::default()
        }));
        let style = StyleDesc {
            border_color: Some("currentColor".to_string()),
            ..Default::default()
        };
        let resolved = Resolved::build(&style, &cascade);
        assert_eq!(
            resolved.base.border_color,
            crate::color::parse_color_rgba("#ff0000").map(Into::into)
        );
        // It read the cascade, so it must not survive a cascade change.
        assert!(resolved.cascade.is_some());
    }

    #[test]
    fn current_color_takes_the_declaration_on_the_element_itself() {
        let style = StyleDesc {
            color: Some("#00ff00".to_string()),
            border_color: Some("currentColor".to_string()),
            ..Default::default()
        };
        // The walk descends before it resolves, so the element's own colour is
        // already in the cascade by the time `currentColor` reads it.
        let cascade = no_variables().descend(Some(&style));
        let resolved = Resolved::build(&style, &cascade);
        assert_eq!(
            resolved.base.border_color,
            crate::color::parse_color_rgba("#00ff00").map(Into::into)
        );
    }

    fn line_height_of(text: &str) -> Option<gpui::DefiniteLength> {
        let style = StyleDesc {
            line_height: Some(crate::style::Numeric::Text(text.to_string())),
            ..Default::default()
        };
        Resolved::build(&style, &no_variables()).base.text.line_height
    }

    #[test]
    fn a_bare_line_height_in_a_string_is_a_multiple_of_the_font_size() {
        // CSS reads `line-height: 1.5` as one and a half times the font size.
        // Reading it as 1.5 pixels would collapse every line onto the last.
        assert_eq!(line_height_of("1.5"), Some(gpui::relative(1.5)));
    }

    #[test]
    fn a_numeric_line_height_is_pixels() {
        // The upstream API reads `lineHeight: 20` as 20 pixels, like React
        // Native. Only a string value gets the CSS multiple reading.
        let numeric = StyleDesc {
            line_height: Some(crate::style::Numeric::Number(20.0)),
            ..Default::default()
        };
        assert_eq!(
            Resolved::build(&numeric, &no_variables()).base.text.line_height,
            Some(gpui::px(20.0).into())
        );
    }

    #[test]
    fn a_line_height_with_a_unit_is_that_length() {
        assert_eq!(line_height_of("24px"), Some(gpui::px(24.0).into()));
        assert_eq!(line_height_of("1.5rem"), Some(gpui::px(24.0).into()));
        assert_eq!(line_height_of("150%"), Some(gpui::relative(1.5)));
    }

    #[test]
    fn a_line_height_of_zero_or_less_declares_nothing() {
        assert_eq!(line_height_of("0"), None);
        assert_eq!(line_height_of("-1"), None);
        assert_eq!(line_height_of("-4px"), None);
    }

    #[test]
    fn calc_reaches_a_length() {
        let style = StyleDesc {
            padding: Some(crate::style::Numeric::Text(
                "calc(var(--spacing) * 6)".to_string(),
            )),
            ..Default::default()
        };
        let cascade = variables(&[("--spacing", "0.25rem")]);
        let resolved = Resolved::build(&style, &cascade);
        assert_eq!(resolved.base.padding.top, Some(gpui::px(24.0).into()));
    }
}

/// The GPUI cursor for a CSS `cursor` keyword. `auto` and unknown words set
/// nothing, so the element keeps the cursor of whatever it sits in.
pub(crate) fn cursor_style(name: &str) -> Option<gpui::CursorStyle> {
    use gpui::CursorStyle::*;
    Some(match name.trim() {
        "default" => Arrow,
        "pointer" => PointingHand,
        "text" => IBeam,
        "vertical-text" => IBeamCursorForVerticalLayout,
        "crosshair" => Crosshair,
        "grab" => OpenHand,
        "grabbing" => ClosedHand,
        "not-allowed" | "no-drop" => OperationNotAllowed,
        "col-resize" => ResizeColumn,
        "row-resize" => ResizeRow,
        "e-resize" => ResizeRight,
        "w-resize" => ResizeLeft,
        "n-resize" => ResizeUp,
        "s-resize" => ResizeDown,
        "ew-resize" => ResizeLeftRight,
        "ns-resize" => ResizeUpDown,
        "nesw-resize" | "ne-resize" | "sw-resize" => ResizeUpRightDownLeft,
        "nwse-resize" | "nw-resize" | "se-resize" => ResizeUpLeftDownRight,
        "alias" => DragLink,
        "copy" => DragCopy,
        "context-menu" => ContextualMenu,
        _ => return None,
    })
}
