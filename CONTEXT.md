# CONTEXT.md

The shared vocabulary for GPUIX. Every term here has one meaning in this
codebase, and module names come from this list.

Most of these words are taken from the CSS specifications. Where a word appears
in a specification, this file uses the specification meaning and nothing else.
That rule matters more than usual here, because GPUIX is being built to match
the specifications 1:1, so a word that drifts costs a reader twice.

## Values and declarations

**Declaration.** One property name and one value. `color: red` is a
declaration. This is the smallest unit the engine moves around.

**Declaration block.** A set of declarations that arrived together from one
source. lightningcss keeps normal and important declarations in two separate
vectors inside one block, so "block" does not mean "all of equal weight".

**Specified value.** A value as written, before anything reads it. Holds
`var()` references and relative units.

**Computed value.** A value after the engine reads custom properties, resolves
relative units against the element and its ancestors, and applies inheritance.
This is what the engine stores and what animation interpolates.

**Used value.** A computed value after layout supplies what was missing, such as
a percentage width that needed a containing block. Layout produces these, not
the cascade.

**Scope.** The custom properties visible to one element. An element reads its
own declarations first and then its ancestors'.

## Selecting and cascading

**Cascade.** The specification algorithm that picks one winning declaration when
several declare the same property. It sorts by origin, importance, layer and
specificity. It is not inheritance.

**Inheritance.** Passing a computed value from an element to its children,
for the properties the specification marks as inherited. A separate mechanism
from the cascade, and easy to confuse with it.

**Cascade level.** One rank the cascade sorts by. GPUIX has four author levels:
normal class, normal inline, important class, important inline. Important
levels reverse the order of the normal ones.

**Specificity.** The three-number weight of a selector, used by the cascade as a
tiebreak. `parcel_selectors` computes it, so GPUIX does not.

**Matching context.** The view of the retained tree that can answer a selector
question. It knows an element's classes, id, attributes, siblings and position.
It exists so the tree stays a data structure and matching lives beside it.

**Condition.** Anything that gates whether a declaration block applies. A state
pseudo-class such as `:hover`, a media query, or a container query. Conditions
are an open set. Nothing in the engine may hardcode a fixed list of them.

**Class channel.** The stylesheet a GPUIX app hands the engine, as CSS text.
Replaces the per-token resolver callback. Tailwind output goes here unchanged.

## Building a frame

**Retained tree.** The element tree GPUIX keeps between frames, mutated by
React through the reconciler. The engine reads it. GPUI does not see it.

**Retained element.** One node of the retained tree.

**Element tree.** The GPUI element tree, rebuilt every frame from the retained
tree. GPUI is immediate mode, so this is thrown away and rebuilt each time.
Do not use this term for the retained tree.

**Frame phase.** One named step in turning the retained tree into an element
tree. The phases are matching, resolving, layout, the container query second
pass, and paint order. They run in a fixed sequence, and container queries make
the sequence run twice.

**Frame walk.** The recursion that turns the retained tree into an element
tree, once per frame. `renderer/frame.rs` owns it, and every frame phase runs
inside it.

**Motion frame.** The values an animation drives for one element on one frame:
`width`, `height`, `top`, `right`, `bottom`, `left`, `borderRadius` and
`opacity`. A motion frame is not a declaration. It reaches the style sink after
the resolved style does, so an animated element keeps its cached resolution.

**Isolated layout tree.** A taffy tree of its own, for laying content out while
the main tree computes. Taffy runs one tree at a time, so an element that sizes
itself from content it has to measure needs a second tree. `IsolatedLayout` in
GPUI holds it, and `AutoHeight` is the element that uses it.

**Resolved style.** The output of the resolve phase for one element: computed
values plus the conditional blocks that paint may still apply. Cached on the
retained element and dropped when the style changes.

**Style sink.** The one trait at the GPUI edge. It takes computed values and
writes them onto a GPUI type. `gpuix-css` defines it, `gpuix-native` implements
it. This trait is the only thing in the engine that knows GPUI exists.

**Wire format.** The JSON shape React sends over napi. `StyleDesc` is the wire
format for the `style` prop. It stops at the crate edge and the engine never
sees it.

## Naming rules

A word from a CSS specification keeps its specification meaning. If code needs a
concept the specifications do not name, give it a name that is clearly not a
specification word.

A module is named for the concept it owns, not for the layer it sits in. Prefer
`cascade` over `style_utils`.
