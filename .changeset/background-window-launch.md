---
'@gpuix/native': minor
'@gpuix/react': minor
---

Launch a window without stealing focus, or with no window at all.

`render()` takes two new window options. `focus: false` opens the window behind whatever app you were typing in, exactly like `open -g`. `show: false` opens nothing at all, so the process boots with a live React tree and an empty screen.

```tsx
render(<App />, { title: 'Notes', focus: false })
```

**Turn this on whenever a coding agent runs your app.** An agent that starts the app to check its work otherwise yanks the window in front of you, mid-sentence, once per iteration. Automation never needs focus: `click()` hits the last painted bounds and `screenshot()` reads the GPU surface, so both work on a background window and even on a `show: false` window that is not on screen. Drive it from the environment so a human run still behaves normally:

```tsx
render(<App />, { focus: process.env.GPUIX_BACKGROUND !== '1' })
```

```ts
const app = await launch({
  command: 'bun',
  args: ['app.tsx'],
  env: { GPUIX_BACKGROUND: '1' },
})
```

`fill()` and `press()` still do not work against `launch()`; the live renderer has no `simulateKeystrokes` and throws `keystrokes are not live yet`. That is unrelated to focus. Use `createTestRoot()` when a check needs typing.

Before this, `init()` called `cx.activate(true)` unconditionally on every platform, so a GPUIX app always jumped to the front on launch. That call is now gated on `focus`. The window flag alone is not enough: on macOS it only decides whether the window becomes *key inside* the app, while activation is what pulls the whole process forward. Both had to change together.

The new `activateWindow()` brings the window forward and focuses it. It is the only way to reveal a `show: false` window.

```tsx
import { useGpuixRequired } from '@gpuix/react'

function Reveal() {
  const renderer = useGpuixRequired()
  return <div onClick={() => renderer.activateWindow?.()}>Show</div>
}
```

Platform support comes straight from GPUI's `WindowParams`:

| Platform | `focus: false` | `show: false` |
| --- | --- | --- |
| macOS | orders in front without becoming key | honored |
| Windows | `SW_SHOWNOACTIVATE` | honored |
| Linux | **ignored** | **ignored** |

The macOS Dock icon still appears. GPUI hardcodes the regular activation policy, so a menu-bar-agent app would need a fork change; nothing upstream configures it today. Use a `launchd` agent for a real background daemon.

Verified against real windows on macOS by comparing the frontmost process id and the AppKit window count before and after launch: `focus: false` left the previous app frontmost with one window on screen, `show: false` reported zero windows, and `activateWindow()` then brought it to one window and frontmost.
