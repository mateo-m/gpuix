---
"@gpuix/native": minor
"@gpuix/react": minor
---

Ease the mix between two gradient stops.

An `<easing-function>` between two colour stops bends the mix, following the
CSSWG proposal in csswg-drafts issue 1332: `linear-gradient(to top, black,
ease-in-out, transparent)`. `ease`, `ease-in`, `ease-out`, `ease-in-out` and
`cubic-bezier()` are read. The shader solves the curve per fragment, so an
eased scrim stays one quad. A straight fade to transparent looks dense near
the solid stop and thin near the clear one; an eased one reads as one smooth
fall-off. The Windows shader also catches up with eight stops and corner
shapes.
