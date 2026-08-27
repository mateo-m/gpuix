//! CSS `scroll-behavior`, `scroll-snap-*` and `scroll-initial-target`.
//!
//! A smooth programmatic scroll is an animation on the scroll offset. The
//! render loop steps every animation once per frame, and a wheel that moves
//! the box away from the animation cancels it, the way a browser does.
//!
//! Scroll snap watches each snap container. While the offset moves, the
//! container is active. When it has rested for `IDLE_SECONDS`, the container
//! picks the nearest snap position among its snap areas and glides to it.
//! `mandatory` always snaps. `proximity` snaps only when the position is
//! within half a viewport. `scroll-snap-stop: always` on an area stops a
//! scroll that would pass over it.
//!
//! `scroll-initial-target: nearest` on an element scrolls its ancestors to
//! it once, on the first frame after the element paints.
//!
//! All state lives in thread locals on the render thread, next to
//! `SCROLL_HANDLES`, so the napi methods reach it without an App context.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::time::Instant;

use gpui::{point, px, Pixels, Point, ScrollHandle};

use crate::motion::{ease, mix, MotionEase};
use crate::retained_tree::RetainedTree;
use crate::style::StyleDesc;

use super::scroll_into_view::{
    axis_delta, scroll_into_view, scroll_margin, scroll_padding, Align, Container,
};

/// How long a smooth scroll takes, in seconds.
const SMOOTH_SECONDS: f64 = 0.3;
/// How long the offset must rest before a snap container snaps. A step
/// under half a pixel does not reset the timer, so the glide starts
/// during the momentum tail of a wheel instead of after it.
const IDLE_SECONDS: f64 = 0.08;

fn smooth_ease() -> MotionEase {
    MotionEase::Name("easeInOut".to_string())
}

/// The `behavior` option of `scrollTo` and `scrollIntoView`. `Auto` reads
/// the `scroll-behavior` of each scroll box.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Behavior {
    Auto,
    Instant,
    Smooth,
}

impl Behavior {
    pub(crate) fn parse(word: Option<&str>) -> Behavior {
        match word.map(str::trim) {
            Some("smooth") => Behavior::Smooth,
            Some("instant") => Behavior::Instant,
            _ => Behavior::Auto,
        }
    }

    /// Whether a scroll of the box with this style moves smoothly.
    pub(crate) fn smooth(self, style: Option<&StyleDesc>) -> bool {
        match self {
            Behavior::Smooth => true,
            Behavior::Instant => false,
            Behavior::Auto => style
                .and_then(|style| style.scroll_behavior.as_deref())
                .is_some_and(|word| word.trim() == "smooth"),
        }
    }
}

/// One running scroll animation.
struct Animation {
    from: Point<Pixels>,
    to: Point<Pixels>,
    /// Set on the first step, so a paused test clock drives it.
    started: Option<Instant>,
    /// The offset the last step wrote. The box sitting anywhere else means
    /// the user took over, which cancels the animation.
    written: Point<Pixels>,
}

/// What a snap container did lately.
struct SnapState {
    /// The offset at the end of the last frame.
    offset: Point<Pixels>,
    /// When the offset last moved, or `None` while the box rests.
    moved_at: Option<Instant>,
    /// The offset where the movement began. `scroll-snap-stop: always`
    /// stops the first such area the scroll passed from here.
    from: Point<Pixels>,
}

thread_local! {
    static ANIMATIONS: RefCell<HashMap<u64, Animation>> = RefCell::new(HashMap::new());
    static SNAP: RefCell<HashMap<u64, SnapState>> = RefCell::new(HashMap::new());
    static INITIAL_DONE: RefCell<HashSet<u64>> = RefCell::new(HashSet::new());
}

/// Glide the box from where it is to `to`. A second call replaces the
/// first, so the glide re-targets rather than queues. The target clamps
/// to the scrollable range, so a glide never runs past the end.
pub(crate) fn animate(id: u64, handle: &ScrollHandle, to: Point<Pixels>) {
    let max = handle.max_offset();
    let to = point(
        to.x.max(-max.x).min(px(0.0)),
        to.y.max(-max.y).min(px(0.0)),
    );
    let from = handle.offset();
    ANIMATIONS.with(|cell| {
        cell.borrow_mut().insert(
            id,
            Animation {
                from,
                to,
                started: None,
                written: from,
            },
        );
    });
}

/// Step every animation and every snap container once. Returns true while
/// anything still moves or waits, so the caller keeps frames coming.
pub(crate) fn frame(
    tree: &RetainedTree,
    handles: &HashMap<u64, ScrollHandle>,
    now: Instant,
) -> bool {
    prune(tree);
    let mut active = step_animations(handles, now);
    active |= initial_targets(tree, handles);
    active |= snap_containers(tree, handles, now);
    active
}

fn prune(tree: &RetainedTree) {
    ANIMATIONS.with(|cell| {
        cell.borrow_mut()
            .retain(|id, _| tree.elements.contains_key(id))
    });
    SNAP.with(|cell| {
        cell.borrow_mut()
            .retain(|id, _| tree.elements.contains_key(id))
    });
    // An id that left the tree re-arms, so a remounted element scrolls into
    // view again.
    INITIAL_DONE.with(|cell| {
        cell.borrow_mut()
            .retain(|id| tree.elements.contains_key(id))
    });
}

fn step_animations(handles: &HashMap<u64, ScrollHandle>, now: Instant) -> bool {
    ANIMATIONS.with(|cell| {
        let mut animations = cell.borrow_mut();
        animations.retain(|id, animation| {
            let Some(handle) = handles.get(id) else {
                return false;
            };
            if handle.offset() != animation.written {
                return false;
            }
            let started = *animation.started.get_or_insert(now);
            let raw = (now - started).as_secs_f64() / SMOOTH_SECONDS;
            if raw >= 1.0 {
                handle.set_offset(animation.to);
                return false;
            }
            let t = ease(raw.max(0.0), &smooth_ease());
            let at = point(
                px(mix(f32::from(animation.from.x) as f64, f32::from(animation.to.x) as f64, t) as f32),
                px(mix(f32::from(animation.from.y) as f64, f32::from(animation.to.y) as f64, t) as f32),
            );
            handle.set_offset(at);
            animation.written = at;
            true
        });
        !animations.is_empty()
    })
}

/// Scroll to every new `scroll-initial-target` element. Returns true while
/// one still waits for its first painted bounds.
fn initial_targets(tree: &RetainedTree, handles: &HashMap<u64, ScrollHandle>) -> bool {
    let mut waiting = false;
    INITIAL_DONE.with(|cell| {
        let mut done = cell.borrow_mut();
        for (&id, element) in &tree.elements {
            let declared = element
                .style
                .as_deref()
                .and_then(|style| style.scroll_initial_target.as_deref())
                .is_some_and(|word| word.trim() == "nearest");
            if !declared || done.contains(&id) {
                continue;
            }
            if crate::automation::get_bounds(id).is_none() {
                // The element has not painted yet. Ask for one more frame
                // and read its bounds then.
                waiting = true;
                continue;
            }
            scroll_into_view(
                tree,
                id,
                Align::Start,
                Align::Nearest,
                Behavior::Auto,
                Container::All,
                |id| handles.get(&id).cloned(),
            );
            done.insert(id);
        }
    });
    waiting
}

/// `mandatory` or `proximity`.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Strictness {
    Mandatory,
    Proximity,
}

/// A parsed `scroll-snap-type`.
#[derive(Clone, Copy)]
struct SnapType {
    x: bool,
    y: bool,
    strictness: Strictness,
}

fn snap_type(style: Option<&StyleDesc>) -> Option<SnapType> {
    let words = style?.scroll_snap_type.as_deref()?;
    let mut parts = words.split_whitespace();
    let (x, y) = match parts.next()? {
        "x" | "inline" => (true, false),
        "y" | "block" => (false, true),
        "both" => (true, true),
        _ => return None,
    };
    let strictness = match parts.next() {
        Some("mandatory") => Strictness::Mandatory,
        _ => Strictness::Proximity,
    };
    Some(SnapType { x, y, strictness })
}

/// A parsed `scroll-snap-align`, as the block word then the inline word.
fn snap_align(style: Option<&StyleDesc>) -> [Option<Align>; 2] {
    let Some(words) = style.and_then(|style| style.scroll_snap_align.as_deref()) else {
        return [None; 2];
    };
    let word = |text: &str| match text {
        "start" => Some(Align::Start),
        "center" => Some(Align::Center),
        "end" => Some(Align::End),
        _ => None,
    };
    let mut parts = words.split_whitespace();
    let block = parts.next().and_then(word);
    match parts.next() {
        Some(inline) => [block, word(inline)],
        None => [block, block],
    }
}

fn snap_stop_always(style: Option<&StyleDesc>) -> bool {
    style
        .and_then(|style| style.scroll_snap_stop.as_deref())
        .is_some_and(|word| word.trim() == "always")
}

/// Watch every snap container and snap the ones that came to rest.
fn snap_containers(
    tree: &RetainedTree,
    handles: &HashMap<u64, ScrollHandle>,
    now: Instant,
) -> bool {
    let mut active = false;
    SNAP.with(|cell| {
        let mut states = cell.borrow_mut();
        for (&id, element) in &tree.elements {
            let Some(snap) = snap_type(element.style.as_deref()) else {
                continue;
            };
            let Some(handle) = handles.get(&id) else {
                continue;
            };
            let animating = ANIMATIONS.with(|cell| cell.borrow().contains_key(&id));
            let offset = handle.offset();
            let state = states.entry(id).or_insert(SnapState {
                offset,
                moved_at: None,
                from: offset,
            });
            if animating {
                // The glide is ours. Track it without arming the idle timer.
                state.offset = offset;
                state.moved_at = None;
                continue;
            }
            if offset != state.offset {
                let step = f32::from(offset.x - state.offset.x)
                    .abs()
                    .max(f32::from(offset.y - state.offset.y).abs());
                if state.moved_at.is_none() {
                    state.from = state.offset;
                    state.moved_at = Some(now);
                } else if step >= 0.5 {
                    state.moved_at = Some(now);
                }
                state.offset = offset;
                if step >= 0.5 {
                    active = true;
                    continue;
                }
                // A step under half a pixel is the momentum tail. Fall
                // through to the idle check, so the snap starts early.
            }
            let Some(moved_at) = state.moved_at else {
                continue;
            };
            if (now - moved_at).as_secs_f64() < IDLE_SECONDS {
                active = true;
                continue;
            }
            state.moved_at = None;
            if let Some(target) = snap_target(tree, id, snap, handle, state.from, handles) {
                if target != offset {
                    animate(id, handle, target);
                    active = true;
                }
            }
        }
    });
    active
}

/// The snap areas of a container: every descendant with a
/// `scroll-snap-align`, without looking inside nested scroll boxes.
fn snap_areas(
    tree: &RetainedTree,
    container: u64,
    handles: &HashMap<u64, ScrollHandle>,
) -> Vec<u64> {
    let mut areas = Vec::new();
    let mut stack: Vec<u64> = tree
        .elements
        .get(&container)
        .map(|element| element.children.clone())
        .unwrap_or_default();
    while let Some(id) = stack.pop() {
        let Some(element) = tree.elements.get(&id) else {
            continue;
        };
        let style = element.style.as_deref();
        if snap_align(style) != [None, None] {
            areas.push(id);
        }
        if handles.contains_key(&id) {
            continue;
        }
        stack.extend(element.children.iter().copied());
    }
    areas
}

/// The rest offset of each snap area on one axis, for a scroll marker
/// group. One offset per area, sorted from the start of the content, so
/// the nth marker stands for the nth stop. Areas that clamp onto the same
/// offset keep their own marker, the way `::scroll-marker` keeps one per
/// element. A container that has not painted yet has no offsets.
pub(crate) fn marker_targets(
    tree: &RetainedTree,
    container: u64,
    handle: &ScrollHandle,
    handles: &HashMap<u64, ScrollHandle>,
    horizontal: bool,
) -> Vec<Pixels> {
    let Some(bounds) = crate::automation::get_bounds(container) else {
        return Vec::new();
    };
    let style = |id: u64| tree.elements.get(&id).and_then(|el| el.style.as_deref());
    let padding = scroll_padding(style(container));
    let (port_start, port_end) = if horizontal {
        (
            bounds.x as f32 + padding[3],
            (bounds.x + bounds.width) as f32 - padding[1],
        )
    } else {
        (
            bounds.y as f32 + padding[0],
            (bounds.y + bounds.height) as f32 - padding[2],
        )
    };
    let offset = handle.offset();
    let max = handle.max_offset();
    let (current, max) = if horizontal {
        (f32::from(offset.x), max.x)
    } else {
        (f32::from(offset.y), max.y)
    };

    let mut targets: Vec<Pixels> = snap_areas(tree, container, handles)
        .into_iter()
        .filter_map(|id| {
            let area = crate::automation::get_bounds(id)?;
            let words = snap_align(style(id));
            let align = if horizontal {
                words[1].or(words[0])
            } else {
                words[0].or(words[1])
            }
            .unwrap_or(Align::Start);
            let margin = scroll_margin(style(id));
            let (start, end) = if horizontal {
                (
                    area.x as f32 - margin[3],
                    (area.x + area.width) as f32 + margin[1],
                )
            } else {
                (
                    area.y as f32 - margin[0],
                    (area.y + area.height) as f32 + margin[2],
                )
            };
            let delta = axis_delta(align, start, end, port_start, port_end);
            Some(px((current - delta).max(-f32::from(max)).min(0.0)))
        })
        .collect();
    targets.sort_by(|a, b| f32::from(*b).total_cmp(&f32::from(*a)));
    targets
}

/// One candidate snap position on one axis.
struct Candidate {
    offset: f32,
    always: bool,
}

/// Where the container should come to rest, or `None` to stay put.
fn snap_target(
    tree: &RetainedTree,
    container: u64,
    snap: SnapType,
    handle: &ScrollHandle,
    from: Point<Pixels>,
    handles: &HashMap<u64, ScrollHandle>,
) -> Option<Point<Pixels>> {
    let bounds = crate::automation::get_bounds(container)?;
    let style = |id: u64| tree.elements.get(&id).and_then(|el| el.style.as_deref());
    let padding = scroll_padding(style(container));
    let port_start = point(bounds.x as f32 + padding[3], bounds.y as f32 + padding[0]);
    let port_end = point(
        (bounds.x + bounds.width) as f32 - padding[1],
        (bounds.y + bounds.height) as f32 - padding[2],
    );

    let offset = handle.offset();
    let max = handle.max_offset();
    let clamp = |value: f32, max: Pixels| value.max(-f32::from(max)).min(0.0);

    let mut on_x: Vec<Candidate> = Vec::new();
    let mut on_y: Vec<Candidate> = Vec::new();
    for id in snap_areas(tree, container, handles) {
        let Some(area) = crate::automation::get_bounds(id) else {
            continue;
        };
        let align = snap_align(style(id));
        let always = snap_stop_always(style(id));
        let margin = scroll_margin(style(id));
        let start = point(area.x as f32 - margin[3], area.y as f32 - margin[0]);
        let end = point(
            (area.x + area.width) as f32 + margin[1],
            (area.y + area.height) as f32 + margin[2],
        );
        if snap.x {
            if let Some(align) = align[1] {
                let delta = axis_delta(align, start.x, end.x, port_start.x, port_end.x);
                on_x.push(Candidate {
                    offset: clamp(f32::from(offset.x) - delta, max.x),
                    always,
                });
            }
        }
        if snap.y {
            if let Some(align) = align[0] {
                let delta = axis_delta(align, start.y, end.y, port_start.y, port_end.y);
                on_y.push(Candidate {
                    offset: clamp(f32::from(offset.y) - delta, max.y),
                    always,
                });
            }
        }
    }

    let x = axis_target(
        &on_x,
        f32::from(offset.x),
        f32::from(from.x),
        (port_end.x - port_start.x) / 2.0,
        snap.strictness,
    );
    let y = axis_target(
        &on_y,
        f32::from(offset.y),
        f32::from(from.y),
        (port_end.y - port_start.y) / 2.0,
        snap.strictness,
    );
    if x.is_none() && y.is_none() {
        return None;
    }
    Some(point(
        px(x.unwrap_or(f32::from(offset.x))),
        px(y.unwrap_or(f32::from(offset.y))),
    ))
}

/// The resting offset on one axis, or `None` to stay put. `proximity`
/// gives up beyond half a viewport. An `always` candidate between the
/// start of the scroll and the nearest position wins, so a long scroll
/// stops at it.
fn axis_target(
    candidates: &[Candidate],
    current: f32,
    from: f32,
    reach: f32,
    strictness: Strictness,
) -> Option<f32> {
    let nearest = candidates
        .iter()
        .min_by(|a, b| {
            (a.offset - current)
                .abs()
                .total_cmp(&(b.offset - current).abs())
        })?
        .offset;
    if strictness == Strictness::Proximity && (nearest - current).abs() > reach.max(0.0) {
        return None;
    }
    let (low, high) = if from <= nearest { (from, nearest) } else { (nearest, from) };
    let stop = candidates
        .iter()
        .filter(|candidate| {
            candidate.always && candidate.offset > low && candidate.offset < high
        })
        .min_by(|a, b| {
            (a.offset - from)
                .abs()
                .total_cmp(&(b.offset - from).abs())
        });
    Some(stop.map_or(nearest, |candidate| candidate.offset))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn candidate(offset: f32) -> Candidate {
        Candidate {
            offset,
            always: false,
        }
    }

    fn always(offset: f32) -> Candidate {
        Candidate {
            offset,
            always: true,
        }
    }

    #[test]
    fn the_nearest_candidate_wins() {
        let candidates = [candidate(0.0), candidate(-100.0), candidate(-200.0)];
        let target = axis_target(&candidates, -80.0, 0.0, 500.0, Strictness::Mandatory);
        assert_eq!(target, Some(-100.0));
    }

    #[test]
    fn proximity_gives_up_beyond_half_a_viewport() {
        let candidates = [candidate(-300.0)];
        assert_eq!(
            axis_target(&candidates, -100.0, -100.0, 150.0, Strictness::Proximity),
            None
        );
        assert_eq!(
            axis_target(&candidates, -200.0, -200.0, 150.0, Strictness::Proximity),
            Some(-300.0)
        );
    }

    #[test]
    fn an_always_stop_catches_a_long_scroll() {
        let candidates = [candidate(0.0), always(-100.0), candidate(-200.0)];
        // A fling from 0 that would rest near -200 stops at the -100 area.
        let target = axis_target(&candidates, -190.0, 0.0, 500.0, Strictness::Mandatory);
        assert_eq!(target, Some(-100.0));
        // A short move from -90 rests at -100 without a fight, because the
        // stop is not strictly between the start and the target.
        let target = axis_target(&candidates, -110.0, -90.0, 500.0, Strictness::Mandatory);
        assert_eq!(target, Some(-100.0));
    }

    #[test]
    fn snap_type_reads_axis_and_strictness() {
        let style = |text: &str| {
            let mut style = StyleDesc::default();
            style.scroll_snap_type = Some(text.to_string());
            style
        };
        let both = snap_type(Some(&style("both mandatory"))).unwrap();
        assert!(both.x && both.y);
        assert!(both.strictness == Strictness::Mandatory);
        let x = snap_type(Some(&style("x"))).unwrap();
        assert!(x.x && !x.y);
        assert!(x.strictness == Strictness::Proximity);
        let block = snap_type(Some(&style("block proximity"))).unwrap();
        assert!(!block.x && block.y);
        assert!(snap_type(Some(&style("none"))).is_none());
    }

    #[test]
    fn snap_align_reads_one_or_two_words() {
        let style = |text: &str| {
            let mut style = StyleDesc::default();
            style.scroll_snap_align = Some(text.to_string());
            style
        };
        assert_eq!(snap_align(Some(&style("start"))), [Some(Align::Start); 2]);
        assert_eq!(
            snap_align(Some(&style("center end"))),
            [Some(Align::Center), Some(Align::End)]
        );
        assert_eq!(snap_align(Some(&style("none"))), [None, None]);
        assert_eq!(snap_align(None), [None, None]);
    }
}
