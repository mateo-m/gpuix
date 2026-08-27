/// View transitions, shown as the push and pop of the iOS Settings app.
///
/// The two screens carry the same `viewTransitionName`, so one
/// `startViewTransition` call animates them as a pair. On a push, the new
/// screen slides in from the right over the old one, and the old one slides
/// 30% of its width to the left. On a pop, the same move runs backwards, and
/// the leaving screen stays on top while it slides out.

import React, { useState } from "react"
import { startViewTransition, useGpuix } from "@gpuix/react"
import type { NativeRenderer, ViewTransitionOptions } from "@gpuix/react"
import { Panel } from "./ui.js"

const PUSH: ViewTransitionOptions = {
  groups: {
    screen: {
      duration: 0.35,
      ease: "easeOut",
      old: { translateX: ["0%", "-30%"] },
      new: { translateX: ["100%", "0%"] },
    },
  },
}

const POP: ViewTransitionOptions = {
  groups: {
    screen: {
      duration: 0.35,
      ease: "easeOut",
      old: { translateX: ["0%", "100%"], onTop: true },
      new: { translateX: ["-30%", "0%"] },
    },
  },
}

const GENERAL_ROWS = ["About", "Software Update", "Storage", "AppleCare", "AirDrop"]
const ROOT_ROWS = ["General", "Display", "Sound", "Focus", "Battery"]

function NavRow({ label, detail, onClick }: {
  label: string
  detail?: string
  onClick?: () => void
}) {
  return (
    <div
      testId={`nav-row-${label}`}
      className={["row items-center px-4 py-3", onClick ? "pointer hover:bg-raised" : ""].join(" ")}
      style={{
        flexShrink: 0,
        justifyContent: "space-between",
        borderBottomWidth: 1,
        borderColor: "var(--color-line)",
      }}
      onClick={onClick}
    >
      <text className="text-sm text-fg">{label}</text>
      <text className="text-sm text-faint">{detail ?? (onClick ? ">" : "")}</text>
    </div>
  )
}

function TitleBar({ title, onBack }: { title: string; onBack?: () => void }) {
  return (
    <div
      className="row items-center gap-2 px-3 py-3"
      style={{ flexShrink: 0, borderBottomWidth: 1, borderColor: "var(--color-line)" }}
    >
      {onBack ? (
        <div testId="nav-back" className="row pointer select-none px-1" onClick={onBack}>
          <text className="text-sm" style={{ color: "var(--color-brand)" }}>{"< Settings"}</text>
        </div>
      ) : null}
      <div className="grow" />
      <text className="text-sm font-semibold text-fg">{title}</text>
      <div className="grow" />
      {onBack ? <div style={{ width: 70 }} /> : null}
    </div>
  )
}

/// One screen of the stack. The name pairs it with the screen it replaces,
/// and the key makes React mount a new element instead of an update in
/// place, the way a real navigation swaps components.
function Screen({ children }: { children: React.ReactNode }) {
  return (
    <div
      className="col w-full h-full"
      style={{ viewTransitionName: "screen", backgroundColor: "var(--color-panel)" }}
    >
      {children}
    </div>
  )
}

function Phone({ renderer }: { renderer: NativeRenderer | null }) {
  const [screen, setScreen] = useState<"root" | "general">("root")
  const go = (next: "root" | "general", options: ViewTransitionOptions) => {
    if (renderer) {
      startViewTransition(renderer, () => setScreen(next), options)
    } else {
      setScreen(next)
    }
  }

  return (
    <div
      className="col rounded border"
      style={{ width: 320, height: 440, flexShrink: 0, overflow: "hidden" }}
    >
      {screen === "root" ? (
        <Screen key="root">
          <TitleBar title="Settings" />
          {ROOT_ROWS.map((label) => (
            <NavRow
              key={label}
              label={label}
              onClick={label === "General" ? () => go("general", PUSH) : undefined}
            />
          ))}
        </Screen>
      ) : (
        <Screen key="general">
          <TitleBar title="General" onBack={() => go("root", POP)} />
          {GENERAL_ROWS.map((label) => (
            <NavRow key={label} label={label} detail="" />
          ))}
        </Screen>
      )}
    </div>
  )
}

export function Navigation() {
  const { renderer } = useGpuix()
  return (
    <Panel
      title="View transitions"
      note="Click General to push its screen. It slides in from the right over the Settings screen, and Settings slides 30% of its width to the left. The back button runs the same move backwards, with the leaving screen on top. One startViewTransition call around the state change drives the whole pair."
    >
      <Phone renderer={renderer ?? null} />
    </Panel>
  )
}
