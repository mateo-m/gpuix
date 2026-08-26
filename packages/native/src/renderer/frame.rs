//! Building one frame of GPUI elements from the retained tree.
//!
//! GPUI is immediate mode, so every frame walks the retained tree and returns a
//! fresh element for each node. This module is that walk. It holds the context
//! the recursion threads through, the virtual list windowing that decides which
//! rows exist this frame, and the builder for each element type.

use std::collections::{HashMap, HashSet};

use super::virtual_list::{window_start_from_element, VirtualListConfig, VirtualListEntry};
use super::{emit_event_full, mouse_button_to_u32, point_to_xy, EventCallback, GpuixView};
use crate::custom_elements::{CustomElementRegistry, CustomRenderContext};
use crate::retained_tree::RetainedTree;
use crate::style::StyleDesc;
use crate::text::{selectable_text, selection_key, SharedSelection};

/// Everything `build_element` threads through the tree.
///
/// Split into a struct because the recursion needs eight-plus shared references
/// and adding one more to every call site is how this file rots. `window` and
/// `cx` stay separate parameters: they are `&mut` and gpui reborrows them.
pub(super) struct BuildCtx<'a> {
    pub tree: &'a RetainedTree,
    pub event_callback: &'a Option<EventCallback>,
    pub focus_handles: &'a HashMap<u64, gpui::FocusHandle>,
    pub scroll_handles: &'a mut HashMap<u64, gpui::ScrollHandle>,
    pub custom_registry: &'a mut CustomElementRegistry,
    pub virtual_lists: &'a mut HashMap<u64, VirtualListEntry>,
    pub motion_states: &'a mut HashMap<u64, crate::motion::MotionState>,
    pub scrollbars: &'a mut super::scrollbar::States,
    pub now: std::time::Instant,
    pub motion_active: &'a mut bool,
    pub selection: SharedSelection,
    /// What this element inherits from its ancestors, resolved the way CSS
    /// inherits it. The renderer's own theme only seeds the root selection
    /// wash. Custom elements resolve their own theme from their `theme` prop.
    pub cascade: crate::inheritance::Inherited,
}

// ── Element builders ─────────────────────────────────────────────────

pub(super) fn build_element(
    id: u64,
    ctx: &mut BuildCtx,
    window: &mut gpui::Window,
    cx: &mut gpui::Context<GpuixView>,
) -> gpui::AnyElement {
    use gpui::IntoElement;

    let Some(element) = ctx.tree.elements.get(&id) else {
        return gpui::Empty.into_any_element();
    };

    // The motion frame for this element, or `None` when it does not animate.
    let motion = if let Some(source) = element.custom_props.get("motion") {
        let state = match ctx.motion_states.entry(id) {
            std::collections::hash_map::Entry::Occupied(entry) => entry.into_mut(),
            std::collections::hash_map::Entry::Vacant(entry) => {
                match crate::motion::MotionState::new(source, ctx.now) {
                    Ok(state) => entry.insert(state),
                    Err(error) => {
                        log::warn!("Invalid motion description for element {id}: {error}");
                        entry.insert(crate::motion::MotionState::invalid(source, ctx.now))
                    }
                }
            }
        };
        if let Err(error) = state.sync(source, ctx.now) {
            log::warn!("Invalid motion update for element {id}: {error}");
        }
        state.is_valid().then(|| {
            let frame = state.frame(ctx.now);
            *ctx.motion_active |= frame.active;
            frame
        })
    } else {
        ctx.motion_states.remove(&id);
        None
    };
    let style = element.style.as_deref();

    // Inheritable style resolves before the element's own style, because a
    // custom property declared here is in scope for the `var()` next to it.
    let parent_cascade = ctx.cascade.clone();
    ctx.cascade = element.descend(&parent_cascade);

    // Resolve the style into a GPUI StyleRefinement. GPUI rebuilds its element
    // tree every frame, so this is the work that used to repeat every frame for
    // styles that had not changed. An animated element reads the same cache,
    // because its motion frame lands on the sink rather than on the style it
    // resolves from.
    let resolved = element.resolved_style(&ctx.cascade);

    let built = match element.element_type.as_str() {
        "div" => {
            ctx.custom_registry.destroy(id);
            build_div(
                element,
                style,
                resolved.clone(),
                motion.as_ref(),
                ctx,
                window,
                cx,
            )
        }
        "text" => {
            ctx.custom_registry.destroy(id);
            build_text(
                element,
                style,
                resolved.clone(),
                motion.as_ref(),
                ctx,
                window,
                cx,
            )
        }
        "virtual-list" => {
            ctx.custom_registry.destroy(id);
            build_virtual_list(element, ctx, window, cx)
        }

        // Polymorphic dispatch for all custom elements.
        custom_type => {
            // Custom renderers take a `StyleDesc` and resolve it themselves, so
            // a motion frame reaches them folded into one. They are the only
            // callers that still pay for that fold.
            let animated = motion.as_ref().map(|frame| {
                let mut declared = element.style.clone().unwrap_or_default();
                frame.style.apply_to(&mut declared);
                declared
            });
            let style = animated.as_deref().or(style);
            let custom_children: Vec<gpui::AnyElement> = element
                .children
                .iter()
                .copied()
                .filter(|child_id| ctx.tree.elements.contains_key(child_id))
                .map(|child_id| build_element(child_id, ctx, window, cx))
                .collect();
            let cascade = ctx.cascade.clone();
            let render_ctx = CustomRenderContext {
                id,
                events: &element.events,
                event_callback: ctx.event_callback,
                focus_handle: ctx.focus_handles.get(&id),
                style,
                children: custom_children,
                selection: ctx.selection.clone(),
                selectable: cascade.selectable(),
                selection_wash: crate::color::to_hsla(cascade.selection_wash()),
                cascade: cascade.clone(),
            };
            ctx.custom_registry
                .render(custom_type, &element.custom_props, render_ctx, window, cx)
        }
    };

    let built = super::auto_height::wrap(id, built, motion.as_ref(), resolved.as_deref());

    ctx.cascade = parent_cascade;
    built
}

fn build_virtual_list(
    element: &crate::retained_tree::RetainedElement,
    ctx: &mut BuildCtx,
    window: &mut gpui::Window,
    cx: &mut gpui::Context<GpuixView>,
) -> gpui::AnyElement {
    use gpui::prelude::*;

    let child_ids: Vec<u64> = element
        .children
        .iter()
        .copied()
        .filter(|child_id| ctx.tree.elements.contains_key(child_id))
        .collect();
    let child_revisions: Vec<u64> = child_ids
        .iter()
        .filter_map(|child_id| {
            ctx.tree
                .elements
                .get(child_id)
                .map(|child| child.subtree_revision)
        })
        .collect();
    let focusable_rows: HashSet<u64> = ctx
        .focus_handles
        .keys()
        .filter_map(|element_id| virtual_row_ancestor(ctx.tree, element.id, *element_id))
        .collect();
    let focused_row = ctx
        .focus_handles
        .iter()
        .find_map(|(element_id, handle)| {
            handle
                .is_focused(window)
                .then(|| virtual_row_ancestor(ctx.tree, element.id, *element_id))
                .flatten()
        })
        .or_else(|| {
            ctx.focus_handles.keys().find_map(|element_id| {
                ctx.tree
                    .elements
                    .get(element_id)
                    .is_some_and(|element| element.auto_focus)
                    .then(|| virtual_row_ancestor(ctx.tree, element.id, *element_id))
                    .flatten()
            })
        });
    let config = VirtualListConfig::from_element(element);
    let window_start = window_start_from_element(element);
    let list_state = match ctx.virtual_lists.entry(element.id) {
        std::collections::hash_map::Entry::Occupied(mut entry) => {
            entry.get_mut().sync(
                config,
                window_start,
                child_ids.clone(),
                child_revisions,
                &focusable_rows,
                cx,
            );
            let entry = entry.into_mut();
            if let Some(row_id) = focused_row.filter(|row_id| !entry.seen_rows.contains(row_id)) {
                if let Some(index) = entry.logical_index_of(row_id) {
                    entry.state.scroll_to(gpui::ListOffset {
                        item_ix: index,
                        offset_in_item: gpui::px(0.0),
                    });
                }
            }
            entry.state.clone()
        }
        std::collections::hash_map::Entry::Vacant(entry) => {
            let row_focus_handles = child_ids
                .iter()
                .map(|id| focusable_rows.contains(id).then(|| cx.focus_handle()))
                .collect();
            let entry = entry.insert(VirtualListEntry::new(
                config,
                window_start,
                child_ids.clone(),
                child_revisions,
                row_focus_handles,
            ));
            if let Some(row_id) = focused_row {
                if let Some(index) = entry.logical_index_of(row_id) {
                    entry.state.scroll_to(gpui::ListOffset {
                        item_ix: index,
                        offset_in_item: gpui::px(0.0),
                    });
                }
            }
            entry.state.clone()
        }
    };

    if element.events.contains("visibleRange") {
        let callback = ctx.event_callback.clone();
        let list_id = element.id;
        list_state.set_scroll_handler(move |event, _window, _cx| {
            emit_event_full(&callback, list_id, "visibleRange", |payload| {
                payload.start_index = Some(event.visible_range.start as f64);
                payload.end_index = Some(event.visible_range.end as f64);
            });
        });
    }

    let list_id = element.id;
    let cascade = ctx.cascade.clone();
    let render_item = cx.processor(move |view, index: usize, window, cx| {
        let Some(child_id) = view
            .virtual_lists
            .get(&list_id)
            .and_then(|entry| entry.child_at(index))
        else {
            return gpui::Empty.into_any_element();
        };
        view.build_virtual_child(list_id, index, child_id, cascade.clone(), window, cx)
    });
    let mut list =
        gpui::list(list_state, render_item).with_sizing_behavior(gpui::ListSizingBehavior::Auto);
    if let Some(resolved) = element.resolved_style(&ctx.cascade) {
        list = crate::style::resolve::apply_resolved(list, &resolved.base);
    }
    list.into_any_element()
}

fn virtual_row_ancestor(tree: &RetainedTree, list_id: u64, element_id: u64) -> Option<u64> {
    let mut current = element_id;
    loop {
        let parent = tree.elements.get(&current)?.parent?;
        if parent == list_id {
            return Some(current);
        }
        current = parent;
    }
}

pub(crate) fn build_div(
    element: &crate::retained_tree::RetainedElement,
    style: Option<&StyleDesc>,
    resolved: Option<std::sync::Arc<crate::style::resolve::Resolved>>,
    motion: Option<&crate::motion::MotionFrame>,
    ctx: &mut BuildCtx,
    window: &mut gpui::Window,
    cx: &mut gpui::Context<GpuixView>,
) -> gpui::AnyElement {
    use gpui::prelude::*;

    let element_id_str = format!("__gpuix_{}", element.id);
    let mut el = gpui::div().id(gpui::SharedString::from(element_id_str));

    if let Some(resolved) = resolved {
        el = crate::style::resolve::apply_resolved(el, &resolved.base);

        // State pseudo-classes. GPUI evaluates these itself, so none of them
        // waits for React. Each takes a closure that receives a
        // StyleRefinement and returns it, and the closure has to be 'static,
        // so each one holds a clone of the shared resolved style.
        //
        // This loop is the dispatcher for states. Adding one is a variant on
        // `State` and an arm here.
        use crate::style::resolve::State;
        // Collect the tags first. Iterating `states` directly would borrow
        // `resolved` across the closures, and cloning the pairs would copy a
        // whole refinement per state to read a one-byte tag.
        let states: Vec<State> = resolved.states.iter().map(|(state, _)| *state).collect();
        for state in states {
            let held = resolved.clone();
            let apply = move |refinement: gpui::StyleRefinement| match held.state(state) {
                Some(declared) => crate::style::resolve::apply_resolved(refinement, declared),
                None => refinement,
            };
            el = match state {
                State::Hover => el.hover(apply),
                State::Active => el.active(apply),
            };
        }
    }

    if let Some(motion) = motion {
        el = crate::style::resolve::apply_motion(el, motion, style);
    }

    if let Some(style) = style {
        if crate::style::should_occlude(style) {
            // BlockMouse (occlude) stops the hit test. The parent scroller
            // then never sees the wheel. In-flow fills must use
            // BlockMouseExceptScroll. Keep occlude for overlays that steal
            // the pointer: absolute, fixed, or pointerEvents: "auto".
            let steal_scroll =
                matches!(style.position.as_deref(), Some("absolute") | Some("fixed"))
                    || style.pointer_events.as_deref() == Some("auto");
            el = if steal_scroll {
                el.occlude()
            } else {
                el.block_mouse_except_scroll()
            };
        }
    }

    // ── Overflow: scroll ─────────────────────────────────────────────
    // overflow_scroll() requires StatefulInteractiveElement (only on Stateful<Div>),
    // so we handle it here rather than in apply_styles (which takes E: Styled).
    //
    // CSS precedence: axis-specific props (overflowX/Y) override the shorthand
    // (overflow). E.g. { overflow: "scroll", overflowY: "hidden" } → scroll X only.
    //
    // overflow-x only works as a flex viewport. Default display is Block, so a
    // wide child fills the parent instead of overflowing. Zed's code-block path:
    // flex + min_w_0 on the scroller, flex_none on the child.
    let mut overflow_x_only = false;
    let mut scrollbar = None;
    if let Some(style) = style {
        // Resolve each axis: axis-specific overrides shorthand.
        let resolved_x = style.overflow_x.as_deref().or(style.overflow.as_deref());
        let resolved_y = style.overflow_y.as_deref().or(style.overflow.as_deref());
        let (resolved_x, resolved_y) = super::scrollbar::used_overflow(resolved_x, resolved_y);

        let needs_scroll_x = super::scrollbar::scrolls(resolved_x);
        let needs_scroll_y = super::scrollbar::scrolls(resolved_y);

        if needs_scroll_x && needs_scroll_y {
            el = el.overflow_scroll();
        } else if needs_scroll_x {
            overflow_x_only = true;
            el = el
                .flex()
                .min_w_0()
                .overflow_x_scroll()
                .restrict_scroll_to_axis();
        } else if needs_scroll_y {
            el = el.overflow_y_scroll();
        }

        // Attach a persistent ScrollHandle when scrolling is enabled.
        // The handle persists across renders (stored in GpuixView::scroll_handles)
        // so GPUI maintains the scroll offset between frames.
        if needs_scroll_x || needs_scroll_y {
            let handle = ctx
                .scroll_handles
                .entry(element.id)
                .or_insert_with(gpui::ScrollHandle::new);
            el = el.track_scroll(handle);

            // The scrollbar. Classic bars reserve a gutter in the layout,
            // which taffy takes as one width for both axes.
            let mode = super::scrollbar::Mode::current(cx);
            if let Some(spec) = super::scrollbar::Spec::from_style(style, mode) {
                let state = ctx.scrollbars.entry(element.id).or_default().clone();
                let reserved = spec.reserved(state.borrow().overflowed);
                let gutter = reserved.x.max(reserved.y);
                if gutter > gpui::px(0.0) {
                    el = el.scrollbar_width(gutter);
                    if spec.both_edges() {
                        let padding = &mut el.style().padding;
                        if needs_scroll_y {
                            padding.left = Some(add_pixels(padding.left, gutter));
                        }
                        if needs_scroll_x {
                            padding.top = Some(add_pixels(padding.top, gutter));
                        }
                    }
                }
                scrollbar = Some(super::scrollbar::Scrollbar::new(
                    spec,
                    handle.clone(),
                    state,
                    ctx.now,
                ));
            }
        } else {
            // Element is no longer scrollable — remove stale handle.
            ctx.scroll_handles.remove(&element.id);
            ctx.scrollbars.remove(&element.id);
        }
    } else {
        // No style at all — remove stale handle if it existed.
        ctx.scroll_handles.remove(&element.id);
        ctx.scrollbars.remove(&element.id);
    }

    // If a FocusHandle was pre-created for this element (by sync_focus_handles),
    // attach it via track_focus. This makes the element focusable — clicking it
    // or tabbing to it gives it keyboard focus. The handle persists across renders
    // because it's stored in GpuixView::focus_handles.
    if style.and_then(|style| style.position.as_deref()).is_none() {
        el = el.relative();
    }
    el = el.child(crate::automation::bounds_tracker(
        element.id,
        selection_start_flag(style),
    ));

    if let Some(handle) = ctx.focus_handles.get(&element.id) {
        el = el.track_focus(handle);
    }
    if let Some(tab_index) = element
        .custom_props
        .get("tabIndex")
        .and_then(|value| value.as_i64())
        .and_then(|index| isize::try_from(index).ok())
    {
        el = el.tab_index(tab_index).tab_stop(tab_index >= 0);
    }

    // Wire up events.
    // Some events (on_hover, on_click) require a stateful element (.id()),
    // which we already set above. Others (on_mouse_down, on_key_down) work
    // on any InteractiveElement.
    for event_type in &element.events {
        let id = element.id;
        let callback = ctx.event_callback.clone();
        match event_type.as_str() {
            // ── Click ────────────────────────────────────────────
            "click" => {
                el = el.on_click(move |click_event, _window, _cx| {
                    emit_event_full(&callback, id, "click", |p| {
                        let (x, y) = point_to_xy(click_event.position());
                        p.x = Some(x);
                        p.y = Some(y);
                        p.modifiers = Some(click_event.modifiers().into());
                        p.click_count = Some(click_event.click_count() as u32);
                        p.is_right_click = Some(click_event.is_right_click());
                    });
                });
            }

            // ── Mouse down (all buttons) ─────────────────────────
            "mouseDown" => {
                // Wire all three buttons so JS gets right-click, middle-click, etc.
                for &button in &[
                    gpui::MouseButton::Left,
                    gpui::MouseButton::Middle,
                    gpui::MouseButton::Right,
                ] {
                    let callback = callback.clone();
                    el = el.on_mouse_down(button, move |mouse_event, _window, _cx| {
                        emit_event_full(&callback, id, "mouseDown", |p| {
                            let (x, y) = point_to_xy(mouse_event.position);
                            p.x = Some(x);
                            p.y = Some(y);
                            p.button = Some(mouse_button_to_u32(mouse_event.button));
                            p.click_count = Some(mouse_event.click_count as u32);
                            p.modifiers = Some(mouse_event.modifiers.into());
                        });
                    });
                }
            }

            // ── Mouse up (all buttons) ───────────────────────────
            "mouseUp" => {
                for &button in &[
                    gpui::MouseButton::Left,
                    gpui::MouseButton::Middle,
                    gpui::MouseButton::Right,
                ] {
                    let callback = callback.clone();
                    el = el.on_mouse_up(button, move |mouse_event, _window, _cx| {
                        emit_event_full(&callback, id, "mouseUp", |p| {
                            let (x, y) = point_to_xy(mouse_event.position);
                            p.x = Some(x);
                            p.y = Some(y);
                            p.button = Some(mouse_button_to_u32(mouse_event.button));
                            p.click_count = Some(mouse_event.click_count as u32);
                            p.modifiers = Some(mouse_event.modifiers.into());
                        });
                    });
                }
            }

            // ── Mouse move ───────────────────────────────────────
            "mouseMove" => {
                el = el.on_mouse_move(move |mouse_event, _window, _cx| {
                    emit_event_full(&callback, id, "mouseMove", |p| {
                        let (x, y) = point_to_xy(mouse_event.position);
                        p.x = Some(x);
                        p.y = Some(y);
                        p.modifiers = Some(mouse_event.modifiers.into());
                        p.pressed_button = mouse_event.pressed_button.map(mouse_button_to_u32);
                    });
                });
            }

            // ── Hover (mouseEnter + mouseLeave) ──────────────────
            // GPUI's on_hover fires with true on enter, false on leave.
            // We split into two distinct event types for the React side.
            "mouseEnter" | "mouseLeave" => {
                // Only wire once even if both mouseEnter and mouseLeave are registered.
                // Check if we already wired on_hover via the other event.
                let has_enter = element.events.contains("mouseEnter");
                let has_leave = element.events.contains("mouseLeave");
                // Wire on first encounter (mouseEnter sorts before mouseLeave).
                if event_type.as_str() == "mouseEnter" || !has_enter {
                    let callback_enter = if has_enter {
                        ctx.event_callback.clone()
                    } else {
                        None
                    };
                    let callback_leave = if has_leave {
                        ctx.event_callback.clone()
                    } else {
                        None
                    };
                    el = el.on_hover(move |&is_hovered, _window, _cx| {
                        if is_hovered {
                            emit_event_full(&callback_enter, id, "mouseEnter", |p| {
                                p.hovered = Some(true);
                            });
                        } else {
                            emit_event_full(&callback_leave, id, "mouseLeave", |p| {
                                p.hovered = Some(false);
                            });
                        }
                    });
                }
            }

            // ── Mouse down outside ───────────────────────────────
            // Fires when the user clicks OUTSIDE this element.
            // Critical for "click outside to close" pattern (dropdowns, modals).
            "mouseDownOutside" => {
                el = el.on_mouse_down_out(move |mouse_event, _window, _cx| {
                    emit_event_full(&callback, id, "mouseDownOutside", |p| {
                        let (x, y) = point_to_xy(mouse_event.position);
                        p.x = Some(x);
                        p.y = Some(y);
                        p.button = Some(mouse_button_to_u32(mouse_event.button));
                        p.modifiers = Some(mouse_event.modifiers.into());
                    });
                });
            }

            // ── Scroll wheel ─────────────────────────────────────
            "scroll" => {
                el = el.on_scroll_wheel(move |scroll_event, _window, _cx| {
                    emit_event_full(&callback, id, "scroll", |p| {
                        let (x, y) = point_to_xy(scroll_event.position);
                        p.x = Some(x);
                        p.y = Some(y);
                        p.modifiers = Some(scroll_event.modifiers.into());
                        p.precise = Some(scroll_event.delta.precise());

                        // Convert ScrollDelta to pixel values.
                        // For Lines delta, we use a default line height of 20px.
                        let line_height = gpui::px(20.0);
                        let pixel_delta = scroll_event.delta.pixel_delta(line_height);
                        p.delta_x = Some(f64::from(f32::from(pixel_delta.x)));
                        p.delta_y = Some(f64::from(f32::from(pixel_delta.y)));

                        p.touch_phase = Some(match scroll_event.touch_phase {
                            gpui::TouchPhase::Started => "started".to_string(),
                            gpui::TouchPhase::Moved => "moved".to_string(),
                            gpui::TouchPhase::Ended => "ended".to_string(),
                            gpui::TouchPhase::Cancelled => "cancelled".to_string(),
                        });
                    });
                });
            }

            // ── Key down ─────────────────────────────────────────
            // Requires .focusable() (set above). Element must be focused
            // (clicked or tabbed to) for these to fire.
            "keyDown" => {
                el = el.on_key_down(move |key_event, _window, _cx| {
                    emit_event_full(&callback, id, "keyDown", |p| {
                        p.key = Some(key_event.keystroke.key.clone());
                        p.key_char = key_event.keystroke.key_char.clone();
                        p.is_held = Some(key_event.is_held);
                        p.modifiers = Some(key_event.keystroke.modifiers.into());
                    });
                });
            }

            // ── Key up ───────────────────────────────────────────
            "keyUp" => {
                el = el.on_key_up(move |key_event, _window, _cx| {
                    emit_event_full(&callback, id, "keyUp", |p| {
                        p.key = Some(key_event.keystroke.key.clone());
                        p.key_char = key_event.keystroke.key_char.clone();
                        p.modifiers = Some(key_event.keystroke.modifiers.into());
                    });
                });
            }

            // ── Focus / Blur ─────────────────────────────────────
            // Event emission is handled by FocusHandle subscriptions
            // set up in GpuixView::sync_focus_handles(). The handle is
            // attached to this element via .track_focus() above.
            "focus" | "blur" => {}

            _ => {}
        }
    }

    // Text content — selectable, same as a <text> leaf.
    if let Some(ref content) = element.content {
        el = el.child(text_content(element.id, content, ctx));
    }

    // Children
    let child_ids: Vec<u64> = element.children.clone();
    for child_id in child_ids {
        let child = build_element(child_id, ctx, window, cx);
        el = if overflow_x_only {
            el.child(gpui::div().flex_none().child(child))
        } else {
            el.child(child)
        };
    }

    // Last, so it paints over the content and takes the mouse first.
    if let Some(scrollbar) = scrollbar {
        el = el.child(scrollbar);
    }

    el.into_any_element()
}

/// `length` plus `extra`. A pixel length adds. Any other unit gives way,
/// because the sum would need the box's size to resolve.
fn add_pixels(length: Option<gpui::DefiniteLength>, extra: gpui::Pixels) -> gpui::DefiniteLength {
    match length {
        Some(gpui::DefiniteLength::Absolute(gpui::AbsoluteLength::Pixels(pixels))) => {
            (pixels + extra).into()
        }
        _ => extra.into(),
    }
}

/// A selectable text run owned by `element_id`. Runs are left to gpui so the
/// text keeps inheriting colour, weight and family from ancestor styles.
fn text_content(element_id: u64, content: &str, ctx: &BuildCtx) -> gpui::AnyElement {
    if !ctx.cascade.selectable() {
        // Still logged: `getPaintedText()` promises every painted string, and a
        // `userSelect: "none"` label is exactly the chrome tests want to assert.
        return crate::text::chrome_text(gpui::SharedString::from(content.to_string()), None);
    }
    let text = crate::text::SelectableText::new(
        gpui::SharedString::from(content.to_string()),
        None,
        selection_key(element_id, 0),
        ctx.selection.clone(),
        crate::color::to_hsla(ctx.cascade.selection_wash()),
    );
    selectable_text(crate::text::SelectableText {
        cursor: text.cursor.filter(|_| !ctx.cascade.cursor_declared()),
        ..text
    })
}

pub(crate) fn build_text(
    element: &crate::retained_tree::RetainedElement,
    style: Option<&StyleDesc>,
    resolved: Option<std::sync::Arc<crate::style::resolve::Resolved>>,
    motion: Option<&crate::motion::MotionFrame>,
    ctx: &mut BuildCtx,
    window: &mut gpui::Window,
    cx: &mut gpui::Context<GpuixView>,
) -> gpui::AnyElement {
    use gpui::prelude::*;

    // Fast path: plain text leaf without style. It still goes through
    // `text_content` so the glyphs land in the selection registry — the old
    // raw-string return was the reason text was not selectable.
    if style.is_none() && motion.is_none() && element.children.is_empty() {
        let content = element.content.clone().unwrap_or_default();
        return gpui::div()
            .relative()
            .child(crate::automation::bounds_tracker(element.id, None))
            .child(text_content(element.id, &content, ctx))
            .into_any_element();
    }

    // The full style set, exactly as `<div>` gets it. `<text>` used to apply a
    // text-only subset, so `padding`, `width` and every layout prop on a text
    // node were silently dropped — a hole with no error and no warning.
    let mut el = gpui::div();
    if let Some(resolved) = resolved.as_ref() {
        el = crate::style::resolve::apply_resolved(el, &resolved.base);
    }
    if let Some(motion) = motion {
        el = crate::style::resolve::apply_motion(el, motion, style);
    }
    if style.and_then(|style| style.position.as_deref()).is_none() {
        el = el.relative();
    }
    el = el.child(crate::automation::bounds_tracker(
        element.id,
        selection_start_flag(style),
    ));

    if let Some(ref content) = element.content {
        el = el.child(text_content(element.id, content, ctx));
    }

    let child_ids: Vec<u64> = element.children.clone();
    for child_id in child_ids {
        el = el.child(build_element(child_id, ctx, window, cx));
    }

    el.into_any_element()
}

/// Explicit `userSelect` on this node. `None` means inherit; the ancestor
/// that set the value already owns the start region.
fn selection_start_flag(style: Option<&StyleDesc>) -> Option<bool> {
    match style.and_then(|style| style.user_select.as_deref()) {
        Some("none") => Some(false),
        Some("text") | Some("auto") => Some(true),
        _ => None,
    }
}
