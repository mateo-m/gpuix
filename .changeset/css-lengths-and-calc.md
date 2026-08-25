---
"@gpuix/native": minor
"@gpuix/react": minor
---

Read `lineHeight` the way CSS reads it, and accept `calc()` in any length.

`lineHeight` used to mean pixels, so `lineHeight: 1.5` set a 1.5 px line. CSS
reads a bare number as a multiple of the font size, and that is what it now
means. A length keeps its unit, so `"24px"` is still 24 px, and `"150%"` and
`1.5` are the same thing. Anything at or below zero declares nothing.

**This changes existing layouts.** A `lineHeight` written as a bare number was
already close to useless at pixel scale, so most of them are small numbers that
now read as multiples. To keep the old result, write the unit: `lineHeight: 20`
becomes `lineHeight: "20px"`.

Every length also takes `calc()`, `min()`, `max()` and `clamp()`, folded by
lightningcss while the value parses. `rem` becomes pixels first, against the
window rem size, so `calc(1rem + 4px)` reaches a single number. This is what
makes the Tailwind spacing scale work, because every step in it is
`calc(var(--spacing) * n)`.
