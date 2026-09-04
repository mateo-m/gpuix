---
'@gpuix/react': minor
---

Export `PublicInstance` for typed host-element refs and add a complete bidirectional chat history example built directly on `<virtual-list>`.

The example demonstrates delayed cursor pagination in both directions, variable-height Safe MDX messages, stable prepend anchoring, bounded page retention, memoized rows, loading indicators, and links that jump to messages outside the current page.

```tsx
<virtual-list
  alignment="bottom"
  estimatedItemHeight={150}
  onVisibleRange={handleVisibleRange}
>
  {messages.map((message) => (
    <MessageRow key={message.id} message={message} />
  ))}
</virtual-list>
```
