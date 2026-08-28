//! GPUIX retained renderer for napi desktop hosts and GPUI's browser platform.
//!
//! Mutation-based API: React's reconciler sends individual mutations
//! (createElement, appendChild, setStyle, etc.) instead of a full JSON tree.
//! Rust maintains a RetainedTree and rebuilds GPUI elements from it each frame.
//!
//! Desktop lifecycle:
//!   const renderer = new GpuixRenderer(eventCallback)
//!   renderer.init({ title: 'My App', width: 800, height: 600 })
//!   renderer.createElement(1, "div")     // mutations from React reconciler
//!   renderer.appendChild(0, 1)
//!   renderer.commitMutations()           // signal batch complete
//!   setTimeout(function loop() {         // drive AppKit on macOS
//!     if (!renderer.tick()) process.exit(0)
//!     setTimeout(loop, 8)
//!   })
#[cfg(any(target_os = "windows", target_os = "linux", target_os = "freebsd"))]
use futures::{channel::mpsc, StreamExt as _};
use gpui::AppContext as _;
#[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
use napi::bindgen_prelude::*;
#[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
use napi::threadsafe_function::{ThreadsafeFunction, ThreadsafeFunctionCallMode};
#[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
use napi_derive::napi;
use std::cell::RefCell;
use std::collections::HashMap;
#[cfg(any(target_os = "macos", target_family = "wasm"))]
use std::rc::Rc;
#[cfg(any(target_os = "windows", target_os = "linux", target_os = "freebsd"))]
use std::sync::mpsc::{sync_channel, RecvTimeoutError, SyncSender};
use std::sync::{Arc, Mutex};
#[cfg(any(target_os = "windows", target_os = "linux", target_os = "freebsd"))]
use std::time::Duration;
#[cfg(all(target_arch = "wasm32", target_os = "unknown"))]
use wasm_bindgen::JsCast as _;

use crate::custom_elements::CustomElementRegistry;
use crate::element_tree::EventPayload;
use crate::retained_tree::RetainedTree;
// Custom elements still style their own sub-parts directly.
pub(crate) use crate::style::resolve::{apply_styles, parse_font_weight};
use crate::text::{selection_frame_reset, SharedSelection};
use crate::theme::Theme;

gpui::actions!(gpuix_focus, [FocusNext, FocusPrevious]);

mod auto_height;
mod batch;
mod frame;
mod virtual_list;

pub use batch::apply_batch_to_tree;
use frame::{build_element, unmounted_virtual_row, BuildCtx};
use virtual_list::VirtualListEntry;

pub(crate) fn init_key_bindings(cx: &mut gpui::App) {
    cx.bind_keys([
        gpui::KeyBinding::new("tab", FocusNext, None),
        gpui::KeyBinding::new("shift-tab", FocusPrevious, None),
    ]);
}

/// The Window menu items act on the focused window, and the root element is the
/// only place in GPUIX that has one. `crate::app_menu` owns everything else.
#[cfg(target_os = "macos")]
fn with_window_menu_actions(root: gpui::Div) -> gpui::Div {
    use crate::app_menu::{CloseWindow, MinimizeWindow, ZoomWindow};
    use gpui::prelude::*;

    root.on_action(|_: &MinimizeWindow, window, _cx| window.minimize_window())
        .on_action(|_: &ZoomWindow, window, _cx| window.zoom_window())
        .on_action(|_: &CloseWindow, window, _cx| window.remove_window())
}

#[cfg(not(target_os = "macos"))]
fn with_window_menu_actions(root: gpui::Div) -> gpui::Div {
    root
}

/// Abstracted event callback shared by desktop, browser, and test renderers.
#[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
pub(crate) type EventCallback = Arc<dyn Fn(EventPayload) + Send + Sync>;
#[cfg(all(target_arch = "wasm32", target_os = "unknown"))]
pub(crate) type EventCallback = Rc<dyn Fn(EventPayload)>;

/// Validate and convert a JS number (f64) to a u64 element ID.
/// JS numbers are f64 — lossless for integers up to 2^53.
fn raw_element_id(id: f64) -> std::result::Result<u64, String> {
    if !id.is_finite() || id < 0.0 || id.fract() != 0.0 || id > 9_007_199_254_740_991.0 {
        return Err(format!("Invalid element id: {id}"));
    }
    Ok(id as u64)
}

#[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
pub(crate) fn to_element_id(id: f64) -> Result<u64> {
    raw_element_id(id).map_err(Error::from_reason)
}

thread_local! {
    #[cfg(target_os = "macos")]
    static MAC_PLATFORM: RefCell<Option<Rc<gpui_macos::MacPlatform>>> = const { RefCell::new(None) };
    #[cfg(target_os = "macos")]
    static GPUI_APP: RefCell<Option<gpui::ApplicationHandle>> = const { RefCell::new(None) };
    #[cfg(target_os = "macos")]
    static GPUI_WINDOW: RefCell<Option<gpui::WindowHandle<GpuixView>>> = const { RefCell::new(None) };
    #[cfg(all(target_arch = "wasm32", target_os = "unknown"))]
    static WEB_APP: RefCell<Option<gpui::ApplicationHandle>> = const { RefCell::new(None) };
    #[cfg(all(target_arch = "wasm32", target_os = "unknown"))]
    static WEB_WINDOW: RefCell<Option<gpui::WindowHandle<GpuixView>>> = const { RefCell::new(None) };
    #[cfg(all(target_arch = "wasm32", target_os = "unknown"))]
    static PENDING_DEBUG_OVERLAY: RefCell<Option<gpui::DebugFrameOverlayMode>> =
        const { RefCell::new(None) };
    /// Shared scroll handles — GpuixView writes here during render(),
    /// platform-local handlers read from here for programmatic scroll control.
    /// ScrollHandle is Rc<RefCell<...>> so its methods (set_offset, offset,
    /// scroll_to_item) work without an App context.
    ///
    /// NOTE: This is a singleton — if multiple renderers/windows coexist,
    /// the last one to render wins. Acceptable for now (single-window only).
    /// TODO: Scope by renderer/window ID when multi-window support is added.
    static SCROLL_HANDLES: RefCell<HashMap<u64, gpui::ScrollHandle>> = RefCell::new(HashMap::new());
    static VIRTUAL_LIST_STATES: RefCell<HashMap<u64, gpui::ListState>> = RefCell::new(HashMap::new());
}

fn parse_debug_frame_overlay_mode_str(
    mode: &str,
) -> std::result::Result<gpui::DebugFrameOverlayMode, String> {
    match mode {
        "hidden" => Ok(gpui::DebugFrameOverlayMode::Hidden),
        "minimal" => Ok(gpui::DebugFrameOverlayMode::Minimal),
        "full" => Ok(gpui::DebugFrameOverlayMode::Full),
        other => Err(format!(
            "Unknown debug frame overlay mode {other:?}. Use hidden, minimal, or full."
        )),
    }
}

#[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
pub(crate) fn parse_debug_frame_overlay_mode(mode: &str) -> Result<gpui::DebugFrameOverlayMode> {
    parse_debug_frame_overlay_mode_str(mode).map_err(Error::from_reason)
}

pub(crate) fn debug_frame_overlay_mode_name(mode: gpui::DebugFrameOverlayMode) -> &'static str {
    match mode {
        gpui::DebugFrameOverlayMode::Hidden => "hidden",
        gpui::DebugFrameOverlayMode::Minimal => "minimal",
        gpui::DebugFrameOverlayMode::Full => "full",
    }
}

#[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
pub(crate) fn debug_frame_overlay_stats_js(
    stats: gpui::DebugFrameOverlayStats,
) -> DebugFrameOverlayStats {
    DebugFrameOverlayStats {
        current_ms: stats.current_ms.map(|ms| ms as f64),
        p90_ms: stats.p90_ms.map(|ms| ms as f64),
        p99_ms: stats.p99_ms.map(|ms| ms as f64),
        max_ms: stats.max_ms.map(|ms| ms as f64),
        frames: stats.frames as f64,
        samples: stats.samples as f64,
    }
}

#[cfg(any(target_os = "windows", target_os = "linux", target_os = "freebsd"))]
fn recv_ui_response<T>(receiver: std::sync::mpsc::Receiver<T>, operation: &str) -> Result<T> {
    match receiver.recv_timeout(Duration::from_secs(2)) {
        Ok(response) => Ok(response),
        Err(RecvTimeoutError::Timeout) => Err(Error::from_reason(format!(
            "Timed out after 2 seconds waiting for {operation}"
        ))),
        Err(RecvTimeoutError::Disconnected) => Err(Error::from_reason(format!(
            "The GPUI UI thread stopped during {operation}"
        ))),
    }
}

#[cfg(target_os = "macos")]
fn update_window<R>(
    update: impl FnOnce(&mut GpuixView, &mut gpui::Window, &mut gpui::Context<GpuixView>) -> R,
) -> Result<R> {
    let window = GPUI_WINDOW
        .with(|window| *window.borrow())
        .ok_or_else(|| Error::from_reason("GPUI window is not initialized"))?;

    GPUI_APP.with(|app| {
        let app = app.borrow();
        let app = app
            .as_ref()
            .ok_or_else(|| Error::from_reason("GPUI application is not initialized"))?;
        app.update(|cx| {
            window
                .update(cx, update)
                .map_err(|error| Error::from_reason(error.to_string()))
        })
    })
}

#[cfg(target_os = "macos")]
fn invalidate_window() -> Result<()> {
    update_window(|_view, window, cx| {
        cx.notify();
        window.refresh();
    })
}

#[cfg(any(target_os = "windows", target_os = "linux", target_os = "freebsd"))]
enum MouseInput {
    Click {
        x: f64,
        y: f64,
        button: u32,
        modifiers: gpui::Modifiers,
    },
    Down {
        x: f64,
        y: f64,
        button: u32,
        modifiers: gpui::Modifiers,
    },
    Up {
        x: f64,
        y: f64,
        button: u32,
        modifiers: gpui::Modifiers,
    },
    Move {
        x: f64,
        y: f64,
        pressed_button: Option<u32>,
        modifiers: gpui::Modifiers,
    },
    Wheel {
        x: f64,
        y: f64,
        delta_x: f64,
        delta_y: f64,
        modifiers: gpui::Modifiers,
    },
}

#[cfg(any(target_os = "windows", target_os = "linux", target_os = "freebsd"))]
enum ClockControl {
    Pause,
    Set(f64),
    FastForward(f64),
    Resume,
}

#[cfg(any(target_os = "windows", target_os = "linux", target_os = "freebsd"))]
enum UiCommand {
    Invalidate,
    SetWindowTitle(String),
    SetDebugFrameOverlay(gpui::DebugFrameOverlayMode),
    CycleDebugFrameOverlay {
        response: SyncSender<String>,
    },
    GetDebugFrameOverlay {
        response: SyncSender<String>,
    },
    GetDebugFrameOverlayStats {
        response: SyncSender<DebugFrameOverlayStats>,
    },
    ResetDebugFrameOverlayStats,
    ScrollTo {
        id: u64,
        x: f32,
        y: f32,
    },
    ScrollToItem {
        id: u64,
        index: usize,
    },
    GetScrollOffset {
        id: u64,
        response: SyncSender<Option<[f64; 2]>>,
    },
    GetAutomationBounds {
        response: SyncSender<HashMap<u64, crate::automation::ElementBounds>>,
    },
    GetWindowSize {
        response: SyncSender<WindowSize>,
    },
    GetElementBounds {
        id: u64,
        response: SyncSender<Option<crate::automation::ElementBounds>>,
    },
    FocusElement(u64),
    ControlClock {
        control: ClockControl,
        response: SyncSender<f64>,
    },
    DispatchMouse {
        input: MouseInput,
        response: SyncSender<std::result::Result<(), String>>,
    },
    #[cfg(all(target_os = "windows", feature = "test-support"))]
    CaptureScreenshot {
        path: String,
        response: SyncSender<std::result::Result<(), String>>,
    },
    Blur,
}

#[cfg(any(target_os = "windows", target_os = "linux", target_os = "freebsd"))]
fn refresh_ui_window(
    window: gpui::WindowHandle<GpuixView>,
    cx: &mut gpui::AsyncApp,
) -> anyhow::Result<()> {
    window.update(cx, |_view, window, cx| {
        cx.notify();
        window.refresh();
    })
}

#[cfg(any(target_os = "windows", target_os = "linux", target_os = "freebsd"))]
async fn run_ui_commands(
    mut commands: mpsc::UnboundedReceiver<UiCommand>,
    window: gpui::WindowHandle<GpuixView>,
    cx: &mut gpui::AsyncApp,
) {
    while let Some(command) = commands.next().await {
        let result = match command {
            UiCommand::Invalidate => refresh_ui_window(window, cx),
            UiCommand::SetWindowTitle(title) => window.update(cx, move |view, window, cx| {
                view.window_title = title;
                cx.notify();
                window.refresh();
            }),
            UiCommand::SetDebugFrameOverlay(mode) => {
                window.update(cx, move |_view, window, _cx| {
                    window.set_debug_frame_overlay_mode(mode);
                })
            }
            UiCommand::CycleDebugFrameOverlay { response } => {
                window.update(cx, move |_view, window, _cx| {
                    window.cycle_debug_frame_overlay_mode();
                    response
                        .send(
                            debug_frame_overlay_mode_name(window.debug_frame_overlay_mode()).into(),
                        )
                        .ok();
                })
            }
            UiCommand::GetDebugFrameOverlay { response } => {
                window.update(cx, move |_view, window, _cx| {
                    response
                        .send(
                            debug_frame_overlay_mode_name(window.debug_frame_overlay_mode()).into(),
                        )
                        .ok();
                })
            }
            UiCommand::GetDebugFrameOverlayStats { response } => {
                window.update(cx, move |_view, window, _cx| {
                    response
                        .send(debug_frame_overlay_stats_js(
                            window.debug_frame_overlay_stats(),
                        ))
                        .ok();
                })
            }
            UiCommand::ResetDebugFrameOverlayStats => window.update(cx, |_view, window, _cx| {
                window.reset_debug_frame_overlay_stats();
            }),
            UiCommand::ScrollTo { id, x, y } => {
                if !VIRTUAL_LIST_STATES.with(|cell| {
                    let states = cell.borrow();
                    let Some(state) = states.get(&id) else {
                        return false;
                    };
                    state.set_offset_from_scrollbar(gpui::point(gpui::px(x), gpui::px(y)));
                    true
                }) {
                    SCROLL_HANDLES.with(|cell| {
                        if let Some(handle) = cell.borrow().get(&id) {
                            handle.set_offset(gpui::point(gpui::px(x), gpui::px(y)));
                        }
                    });
                }
                refresh_ui_window(window, cx)
            }
            UiCommand::ScrollToItem { id, index } => {
                if !VIRTUAL_LIST_STATES.with(|cell| {
                    let states = cell.borrow();
                    let Some(state) = states.get(&id) else {
                        return false;
                    };
                    state.scroll_to(gpui::ListOffset {
                        item_ix: index,
                        offset_in_item: gpui::px(0.0),
                    });
                    true
                }) {
                    SCROLL_HANDLES.with(|cell| {
                        if let Some(handle) = cell.borrow().get(&id) {
                            handle.scroll_to_item(index);
                        }
                    });
                }
                refresh_ui_window(window, cx)
            }
            UiCommand::GetScrollOffset { id, response } => {
                let offset = VIRTUAL_LIST_STATES
                    .with(|cell| {
                        cell.borrow().get(&id).map(|state| {
                            let offset = state.scroll_px_offset_for_scrollbar();
                            [
                                f64::from(f32::from(offset.x)),
                                f64::from(f32::from(offset.y)),
                            ]
                        })
                    })
                    .or_else(|| {
                        SCROLL_HANDLES.with(|cell| {
                            cell.borrow().get(&id).map(|handle| {
                                let offset = handle.offset();
                                [
                                    f64::from(f32::from(offset.x)),
                                    f64::from(f32::from(offset.y)),
                                ]
                            })
                        })
                    });
                response.send(offset).ok();
                Ok(())
            }
            UiCommand::GetWindowSize { response } => window.update(cx, move |_view, window, _cx| {
                let size = window.viewport_size();
                response
                    .send(WindowSize {
                        width: f32::from(size.width) as f64,
                        height: f32::from(size.height) as f64,
                    })
                    .ok();
            }),
            UiCommand::GetAutomationBounds { response } => {
                window.update(cx, move |_view, window, cx| {
                    cx.notify();
                    window.refresh();
                    window.on_next_frame(move |_window, _cx| {
                        response.send(crate::automation::all_bounds()).ok();
                    });
                })
            }
            UiCommand::GetElementBounds { id, response } => {
                window.update(cx, move |_view, window, cx| {
                    cx.notify();
                    window.refresh();
                    window.on_next_frame(move |_window, _cx| {
                        response.send(crate::automation::get_bounds(id)).ok();
                    });
                })
            }
            UiCommand::FocusElement(id) => window.update(cx, move |view, window, cx| {
                view.reveal_virtual_list_ancestor(id);
                if let Some(handle) = view.focus_handles.get(&id) {
                    handle.focus(window, cx);
                }
                cx.notify();
                window.refresh();
            }),
            UiCommand::ControlClock { control, response } => {
                window.update(cx, move |view, _window, cx| {
                    let now_ms = match control {
                        ClockControl::Pause => view.clock.pause(),
                        ClockControl::Set(now_ms) => view.clock.set_ms(now_ms),
                        ClockControl::FastForward(delta_ms) => view.clock.fast_forward_ms(delta_ms),
                        ClockControl::Resume => view.clock.resume(),
                    };
                    cx.notify();
                    response.send(now_ms).ok();
                })
            }
            UiCommand::DispatchMouse { input, response } => {
                let result = window.update(cx, move |_view, window, cx| match input {
                    MouseInput::Click {
                        x,
                        y,
                        button,
                        modifiers,
                    } => {
                        crate::automation::dispatch_click(window, cx, x, y, button, modifiers);
                    }
                    MouseInput::Down {
                        x,
                        y,
                        button,
                        modifiers,
                    } => {
                        crate::automation::dispatch_mouse_down(window, cx, x, y, button, modifiers);
                    }
                    MouseInput::Up {
                        x,
                        y,
                        button,
                        modifiers,
                    } => {
                        crate::automation::dispatch_mouse_up(window, cx, x, y, button, modifiers);
                    }
                    MouseInput::Move {
                        x,
                        y,
                        pressed_button,
                        modifiers,
                    } => {
                        crate::automation::dispatch_mouse_move(
                            window,
                            cx,
                            x,
                            y,
                            pressed_button,
                            modifiers,
                        );
                    }
                    MouseInput::Wheel {
                        x,
                        y,
                        delta_x,
                        delta_y,
                        modifiers,
                    } => {
                        crate::automation::dispatch_scroll_wheel(
                            window, cx, x, y, delta_x, delta_y, modifiers,
                        );
                    }
                });
                response
                    .send(
                        result
                            .as_ref()
                            .map(|_| ())
                            .map_err(|error| format!("{error:#}")),
                    )
                    .ok();
                result
            }
            #[cfg(all(target_os = "windows", feature = "test-support"))]
            UiCommand::CaptureScreenshot { path, response } => {
                let error_response = response.clone();
                let result = window.update(cx, move |_view, window, cx| {
                    cx.notify();
                    window.refresh();
                    window.on_next_frame(move |window, _cx| {
                        let result = window
                            .render_to_image()
                            .map_err(|error| format!("Screenshot capture failed: {error}"))
                            .and_then(|image| {
                                image
                                    .save(&path)
                                    .map_err(|error| format!("Failed to save screenshot: {error}"))
                            });
                        response.send(result).ok();
                    });
                });
                if let Err(error) = &result {
                    error_response.send(Err(format!("{error:#}"))).ok();
                }
                result
            }
            UiCommand::Blur => window.update(cx, |_view, window, _cx| window.blur()),
        };
        if let Err(error) = result {
            log::error!("Failed to handle GPUI UI command: {error:#}");
        }
    }
    cx.update(|cx| cx.quit());
}

#[cfg(any(target_os = "windows", target_os = "linux", target_os = "freebsd"))]
fn panic_message(payload: Box<dyn std::any::Any + Send>) -> String {
    payload
        .downcast_ref::<&str>()
        .map(|message| (*message).to_string())
        .or_else(|| payload.downcast_ref::<String>().cloned())
        .unwrap_or_else(|| "unknown panic".to_string())
}

/// The main GPUI renderer exposed to Node.js.
#[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
#[napi]
pub struct GpuixRenderer {
    event_callback: Mutex<Option<Arc<ThreadsafeFunction<EventPayload>>>>,
    tree: Arc<Mutex<RetainedTree>>,
    initialized: Arc<Mutex<bool>>,
    /// Shared with GpuixView so napi methods can read the live selection
    /// without an App context. Paint and napi calls can use different threads.
    selection: SharedSelection,
    #[cfg(any(target_os = "windows", target_os = "linux", target_os = "freebsd"))]
    ui_commands: Mutex<Option<mpsc::UnboundedSender<UiCommand>>>,
}

#[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
#[napi]
impl GpuixRenderer {
    fn event_callback_for_view(&self) -> Option<EventCallback> {
        self.event_callback.lock().unwrap().clone().map(|tsf| {
            Arc::new(move |payload: EventPayload| {
                tsf.call(Ok(payload), ThreadsafeFunctionCallMode::NonBlocking);
            }) as EventCallback
        })
    }

    #[cfg(any(target_os = "windows", target_os = "linux", target_os = "freebsd"))]
    fn send_ui_command(&self, command: UiCommand) -> Result<()> {
        self.ui_commands
            .lock()
            .unwrap()
            .as_ref()
            .ok_or_else(|| Error::from_reason("GPUI application is not initialized"))?
            .unbounded_send(command)
            .map_err(|_| Error::from_reason("The GPUI UI thread is not running"))
    }

    #[cfg(any(target_os = "windows", target_os = "linux", target_os = "freebsd"))]
    fn dispatch_mouse_input(&self, input: MouseInput) -> Result<()> {
        let (response_sender, response_receiver) = sync_channel(1);
        self.send_ui_command(UiCommand::DispatchMouse {
            input,
            response: response_sender,
        })?;
        recv_ui_response(response_receiver, "the GPUI UI command")?.map_err(Error::from_reason)
    }

    fn automation_bounds(&self) -> Result<HashMap<u64, crate::automation::ElementBounds>> {
        #[cfg(target_os = "macos")]
        return Ok(crate::automation::all_bounds());

        #[cfg(any(target_os = "windows", target_os = "linux", target_os = "freebsd"))]
        {
            let (response, receiver) = sync_channel(1);
            self.send_ui_command(UiCommand::GetAutomationBounds { response })?;
            return recv_ui_response(receiver, "the automation bounds query");
        }

        #[cfg(not(any(
            target_os = "macos",
            target_os = "windows",
            target_os = "linux",
            target_os = "freebsd"
        )))]
        Err(Error::from_reason("Unsupported operating system"))
    }

    fn element_bounds(&self, id: u64) -> Result<Option<crate::automation::ElementBounds>> {
        #[cfg(target_os = "macos")]
        return Ok(crate::automation::get_bounds(id));

        #[cfg(any(target_os = "windows", target_os = "linux", target_os = "freebsd"))]
        {
            let (response, receiver) = sync_channel(1);
            self.send_ui_command(UiCommand::GetElementBounds { id, response })?;
            return recv_ui_response(receiver, "the element bounds query");
        }

        #[cfg(not(any(
            target_os = "macos",
            target_os = "windows",
            target_os = "linux",
            target_os = "freebsd"
        )))]
        {
            let _ = id;
            Err(Error::from_reason("Unsupported operating system"))
        }
    }

    #[cfg(any(target_os = "windows", target_os = "linux", target_os = "freebsd"))]
    fn control_clock(&self, control: ClockControl) -> Result<f64> {
        let (response, receiver) = sync_channel(1);
        self.send_ui_command(UiCommand::ControlClock { control, response })?;
        recv_ui_response(receiver, "the automation clock command")
    }

    fn request_invalidate(&self) -> Result<()> {
        #[cfg(target_os = "macos")]
        return invalidate_window();

        #[cfg(any(target_os = "windows", target_os = "linux", target_os = "freebsd"))]
        return self.send_ui_command(UiCommand::Invalidate);

        #[cfg(not(any(
            target_os = "macos",
            target_os = "windows",
            target_os = "linux",
            target_os = "freebsd"
        )))]
        Err(Error::from_reason(
            "The production GPUIX renderer does not support this operating system",
        ))
    }

    #[napi(constructor)]
    pub fn new(event_callback: Option<ThreadsafeFunction<EventPayload>>) -> Self {
        let _ = env_logger::try_init();
        Self {
            event_callback: Mutex::new(event_callback.map(Arc::new)),
            tree: Arc::new(Mutex::new(RetainedTree::new())),
            initialized: Arc::new(Mutex::new(false)),
            selection: SharedSelection::default(),
            #[cfg(any(target_os = "windows", target_os = "linux", target_os = "freebsd"))]
            ui_commands: Mutex::new(None),
        }
    }

    /// Initialize GPUI using the native event-loop architecture for this OS.
    #[napi]
    pub fn init(&self, options: Option<WindowOptions>) -> Result<()> {
        #[cfg(not(any(
            target_os = "macos",
            target_os = "windows",
            target_os = "linux",
            target_os = "freebsd"
        )))]
        {
            let _ = options;
            return Err(Error::from_reason(
                "The production GPUIX renderer does not support this operating system",
            ));
        }

        #[cfg(target_os = "macos")]
        return self.init_macos(options);

        #[cfg(any(target_os = "windows", target_os = "linux", target_os = "freebsd"))]
        return self.init_threaded(options);
    }

    #[cfg(target_os = "macos")]
    fn init_macos(&self, options: Option<WindowOptions>) -> Result<()> {
        let options = options.unwrap_or_default();

        {
            let initialized = self.initialized.lock().unwrap();
            if *initialized {
                return Err(Error::from_reason("Renderer is already initialized"));
            }
        }
        if MAC_PLATFORM.with(|platform| platform.borrow().is_some()) {
            return Err(Error::from_reason(
                "A GPUI application already exists on this thread",
            ));
        }

        let width = options.width.unwrap_or(800.0);
        let height = options.height.unwrap_or(600.0);
        let title = options.title.clone().unwrap_or_else(|| "GPUIX".to_string());
        let app_name = options.app_name.clone().unwrap_or_else(|| title.clone());
        let window_options = options.clone();

        let platform = Rc::new(gpui_macos::MacPlatform::new_embedded());

        let tree = self.tree.clone();
        let callback = self.event_callback_for_view();

        let selection = self.selection.clone();
        let opened_window = Rc::new(RefCell::new(None));
        let startup_error = Rc::new(RefCell::new(None));
        let opened_window_for_app = opened_window.clone();
        let startup_error_for_app = startup_error.clone();
        // bun/node is not a .app. A Dock icon with no window cannot relaunch.
        // Last window close quits AppKit; tick() returns false and JS exits.
        let app = gpui::Application::with_platform(platform.clone())
            .with_quit_mode(gpui::QuitMode::LastWindowClosed);
        let app_handle = app.run_embedded(move |cx: &mut gpui::App| {
            init_key_bindings(cx);
            crate::custom_elements::input::init(cx);
            // After the other bindings: `set_menus` reads key equivalents out of
            // the keymap, so every binding must exist before it runs.
            crate::app_menu::init(&app_name, cx);
            let bounds = gpui::Bounds::centered(
                None,
                gpui::size(gpui::px(width as f32), gpui::px(height as f32)),
                cx,
            );

            match cx.open_window(
                to_gpui_window_options(&window_options, bounds),
                |_window, cx| {
                    cx.new(|_cx| {
                        GpuixView::new(tree.clone(), callback.clone(), title, selection.clone())
                    })
                },
            ) {
                Ok(window_handle) => {
                    *opened_window_for_app.borrow_mut() = Some(window_handle);
                    cx.activate(true);
                }
                Err(error) => {
                    *startup_error_for_app.borrow_mut() = Some(error.to_string());
                }
            }
        });

        let startup_result = match startup_error.borrow_mut().take() {
            Some(error) => Err(Error::from_reason(format!(
                "Failed to open the GPUI window: {error}"
            ))),
            None => opened_window
                .borrow_mut()
                .take()
                .ok_or_else(|| Error::from_reason("GPUI did not open the application window")),
        };
        let window_handle = match startup_result {
            Ok(window_handle) => window_handle,
            Err(error) => {
                app_handle.update(|cx| cx.quit());
                if platform.pump_events() {
                    MAC_PLATFORM.with(|stored| {
                        *stored.borrow_mut() = Some(platform.clone());
                    });
                }
                return Err(error);
            }
        };

        MAC_PLATFORM.with(|stored| {
            *stored.borrow_mut() = Some(platform);
        });
        GPUI_APP.with(|a| {
            *a.borrow_mut() = Some(app_handle);
        });
        GPUI_WINDOW.with(|w| {
            *w.borrow_mut() = Some(window_handle);
        });

        *self.initialized.lock().unwrap() = true;
        self.event_callback.lock().unwrap().take();
        Ok(())
    }

    #[cfg(any(target_os = "windows", target_os = "linux", target_os = "freebsd"))]
    fn init_threaded(&self, options: Option<WindowOptions>) -> Result<()> {
        let options = options.unwrap_or_default();
        if *self.initialized.lock().unwrap() {
            return Err(Error::from_reason("Renderer is already initialized"));
        }

        let width = options.width.unwrap_or(800.0);
        let height = options.height.unwrap_or(600.0);
        let title = options.title.clone().unwrap_or_else(|| "GPUIX".to_string());
        let window_options = options.clone();
        let tree = self.tree.clone();
        let selection = self.selection.clone();
        let callback = self.event_callback_for_view();
        let (command_sender, command_receiver) = mpsc::unbounded();
        let (startup_sender, startup_receiver) = sync_channel(1);
        let exit_startup_sender = startup_sender.clone();

        std::thread::Builder::new()
            .name("gpuix-ui".to_string())
            .spawn(move || {
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    gpui_platform::application().run(move |cx| {
                        init_key_bindings(cx);
                        crate::custom_elements::input::init(cx);
                        let bounds = gpui::Bounds::centered(
                            None,
                            gpui::size(gpui::px(width as f32), gpui::px(height as f32)),
                            cx,
                        );
                        let window = match cx.open_window(
                            to_gpui_window_options(&window_options, bounds),
                            |_window, cx| {
                                cx.new(|_cx| {
                                    GpuixView::new(tree, callback, title, selection)
                                })
                            },
                        ) {
                            Ok(window) => window,
                            Err(error) => {
                                startup_sender
                                    .send(Err(format!("Failed to open the GPUI window: {error}")))
                                    .ok();
                                cx.quit();
                                return;
                            }
                        };

                        cx.spawn(async move |cx| {
                            run_ui_commands(command_receiver, window, cx).await;
                        })
                        .detach();
                        cx.activate(true);
                        startup_sender.send(Ok(())).ok();
                    });
                }));

                let error = match result {
                    Ok(()) => {
                        "The GPUI event loop exited before initialization completed".to_string()
                    }
                    Err(payload) => format!(
                        "The GPUI UI thread panicked during initialization: {}",
                        panic_message(payload)
                    ),
                };
                exit_startup_sender.try_send(Err(error)).ok();
            })
            .map_err(|error| {
                Error::from_reason(format!("Failed to spawn the GPUI UI thread: {error}"))
            })?;

        startup_receiver
            .recv()
            .map_err(|_| Error::from_reason("The GPUI UI thread stopped during initialization"))?
            .map_err(Error::from_reason)?;

        *self.ui_commands.lock().unwrap() = Some(command_sender);
        *self.initialized.lock().unwrap() = true;
        self.event_callback.lock().unwrap().take();
        Ok(())
    }

    // ── Mutation API ─────────────────────────────────────────────────

    #[napi]
    pub fn create_element(&self, id: f64, element_type: String) -> Result<()> {
        let id = to_element_id(id)?;
        let mut tree = self.tree.lock().unwrap();
        tree.create_element(id, element_type);
        Ok(())
    }

    /// Destroy an element and all descendants. Returns array of destroyed IDs
    /// so JS can clean up event handlers for the entire subtree.
    #[napi]
    pub fn destroy_element(&self, id: f64) -> Result<Vec<f64>> {
        let id = to_element_id(id)?;
        let mut tree = self.tree.lock().unwrap();
        let destroyed = tree.destroy_element(id);
        Ok(destroyed.iter().map(|&id| id as f64).collect())
    }

    #[napi]
    pub fn append_child(&self, parent_id: f64, child_id: f64) -> Result<()> {
        let parent_id = to_element_id(parent_id)?;
        let child_id = to_element_id(child_id)?;
        let mut tree = self.tree.lock().unwrap();
        tree.append_child(parent_id, child_id);
        Ok(())
    }

    #[napi]
    pub fn remove_child(&self, parent_id: f64, child_id: f64) -> Result<()> {
        let parent_id = to_element_id(parent_id)?;
        let child_id = to_element_id(child_id)?;
        let mut tree = self.tree.lock().unwrap();
        tree.remove_child(parent_id, child_id);
        Ok(())
    }

    #[napi]
    pub fn insert_before(&self, parent_id: f64, child_id: f64, before_id: f64) -> Result<()> {
        let parent_id = to_element_id(parent_id)?;
        let child_id = to_element_id(child_id)?;
        let before_id = to_element_id(before_id)?;
        let mut tree = self.tree.lock().unwrap();
        tree.insert_before(parent_id, child_id, before_id);
        Ok(())
    }

    #[napi]
    pub fn set_style(&self, id: f64, style_json: String) -> Result<()> {
        let id = to_element_id(id)?;
        self.tree
            .lock()
            .unwrap()
            .set_style_json(id, style_json.as_bytes())
            .map_err(|error| Error::from_reason(format!("Failed to parse style: {error}")))
    }

    #[napi]
    pub fn set_text(&self, id: f64, content: String) -> Result<()> {
        let id = to_element_id(id)?;
        let mut tree = self.tree.lock().unwrap();
        tree.set_text(id, content);
        Ok(())
    }

    #[napi]
    pub fn set_event_listener(&self, id: f64, event_type: String, has_handler: bool) -> Result<()> {
        let id = to_element_id(id)?;
        let mut tree = self.tree.lock().unwrap();
        tree.set_event_listener(id, event_type, has_handler);
        Ok(())
    }

    /// Set the root element (called from appendChildToContainer).
    #[napi]
    pub fn set_root(&self, id: f64) -> Result<()> {
        let id = to_element_id(id)?;
        let mut tree = self.tree.lock().unwrap();
        tree.root_id = Some(id);
        Ok(())
    }

    /// Set a custom prop on an element (for non-div/text elements like input, editor, diff).
    /// Key is the prop name, value is JSON-encoded.
    #[napi]
    pub fn set_custom_prop(&self, id: f64, key: String, value_json: String) -> Result<()> {
        let id = to_element_id(id)?;
        let value: serde_json::Value = serde_json::from_str(&value_json)
            .map_err(|e| Error::from_reason(format!("Failed to parse custom prop value: {}", e)))?;
        let mut tree = self.tree.lock().unwrap();
        tree.set_custom_prop(id, key, value);
        Ok(())
    }

    /// Get a custom prop value from an element. Returns JSON string or null.
    #[napi]
    pub fn get_custom_prop(&self, id: f64, key: String) -> Result<Option<String>> {
        let id = to_element_id(id)?;
        let tree = self.tree.lock().unwrap();
        Ok(tree
            .get_custom_prop(id, &key)
            .map(|v| serde_json::to_string(v).unwrap_or_default()))
    }

    /// Signal that a batch of mutations is complete. Triggers re-render.
    #[napi]
    pub fn commit_mutations(&self) -> Result<()> {
        self.request_invalidate()
    }

    /// Apply a batch of mutations in a single FFI call.
    ///
    /// Accepts a JSON array of mutation tuples. Each tuple is an array where
    /// the first element is the operation name (string) and remaining elements
    /// are the arguments:
    ///
    ///   ["createElement",    id, "type"]
    ///   ["destroyElement",   id]
    ///   ["appendChild",      parentId, childId]
    ///   ["removeChild",      parentId, childId]
    ///   ["insertBefore",     parentId, childId, beforeId]
    ///   ["setStyle",         id, { ...style } | "{styleJson}"]
    ///   ["setText",          id, "content"]
    ///   ["setEventListener", id, "eventType", true|false]
    ///   ["setRoot",          id]
    ///   ["setCustomProp",      id, "key", value | "{valueJson}"]
    ///   ["setCustomPropValue", id, "key", value]
    ///
    /// Returns accumulated destroyed IDs from all destroyElement ops.
    /// Acquires the tree mutex ONCE for the entire batch.
    #[napi]
    pub fn apply_batch(&self, json: String) -> Result<Vec<f64>> {
        let mut tree = self.tree.lock().unwrap();
        let destroyed =
            apply_batch_to_tree(&mut tree, json.as_bytes()).map_err(Error::from_reason)?;
        drop(tree);
        self.request_invalidate()?;
        Ok(destroyed)
    }

    // ── Frame loop ───────────────────────────────────────────────────

    /// Pump the native event loop. Returns false after the last window closes.
    #[napi]
    pub fn tick(&self) -> Result<bool> {
        let initialized = *self.initialized.lock().unwrap();
        if !initialized {
            return Err(Error::from_reason(
                "Renderer not initialized. Call init() first.",
            ));
        }

        #[cfg(target_os = "macos")]
        {
            let running = MAC_PLATFORM.with(|p| {
                p.borrow()
                    .as_ref()
                    .map(|platform| platform.pump_events())
                    .unwrap_or(false)
            });
            return Ok(running);
        }

        #[cfg(any(target_os = "windows", target_os = "linux", target_os = "freebsd"))]
        return Ok(true);

        #[cfg(not(any(
            target_os = "macos",
            target_os = "windows",
            target_os = "linux",
            target_os = "freebsd"
        )))]
        Err(Error::from_reason(
            "The production GPUIX renderer does not support this operating system",
        ))
    }

    #[napi]
    pub fn is_initialized(&self) -> bool {
        *self.initialized.lock().unwrap()
    }

    /// Whether JavaScript must drive the native event loop with tick().
    #[napi]
    pub fn requires_tick(&self) -> bool {
        cfg!(target_os = "macos")
    }

    /// The paintable size of the window in logical pixels, excluding any
    /// platform title bar. This used to answer a hardcoded 800x600, so anything
    /// that turned a mouse position into layout coordinates pointed at the
    /// wrong place on every window that was not exactly that size.
    #[napi]
    pub fn get_window_size(&self) -> Result<WindowSize> {
        #[cfg(target_os = "macos")]
        return update_window(|_view, window, _cx| {
            let size = window.viewport_size();
            WindowSize {
                width: f32::from(size.width) as f64,
                height: f32::from(size.height) as f64,
            }
        });

        #[cfg(any(target_os = "windows", target_os = "linux", target_os = "freebsd"))]
        {
            let (response, receiver) = sync_channel(1);
            self.send_ui_command(UiCommand::GetWindowSize { response })?;
            return recv_ui_response(receiver, "the window size query");
        }

        #[cfg(not(any(
            target_os = "macos",
            target_os = "windows",
            target_os = "linux",
            target_os = "freebsd"
        )))]
        Err(Error::from_reason(
            "The production GPUIX renderer does not support this operating system",
        ))
    }

    #[napi]
    pub fn get_window_insets(&self) -> Result<WindowInsets> {
        #[cfg(target_os = "macos")]
        return update_window(|_view, window, _cx| WindowInsets::from_gpui(window.insets()));

        #[cfg(not(target_os = "macos"))]
        Ok(WindowInsets::default())
    }

    /// `"hidden"` | `"minimal"` | `"full"`. Paints into the scene after layout.
    #[napi]
    pub fn set_debug_frame_overlay(&self, mode: String) -> Result<String> {
        let mode = parse_debug_frame_overlay_mode(&mode)?;
        #[cfg(target_os = "macos")]
        return update_window(move |_view, window, _cx| {
            window.set_debug_frame_overlay_mode(mode);
            debug_frame_overlay_mode_name(window.debug_frame_overlay_mode()).to_string()
        });

        #[cfg(any(target_os = "windows", target_os = "linux", target_os = "freebsd"))]
        {
            self.send_ui_command(UiCommand::SetDebugFrameOverlay(mode))?;
            return self.debug_frame_overlay_mode();
        }

        #[cfg(not(any(
            target_os = "macos",
            target_os = "windows",
            target_os = "linux",
            target_os = "freebsd"
        )))]
        Err(Error::from_reason("Unsupported operating system"))
    }

    /// Hidden → minimal → full → hidden.
    #[napi]
    pub fn cycle_debug_frame_overlay(&self) -> Result<String> {
        #[cfg(target_os = "macos")]
        return update_window(move |_view, window, _cx| {
            window.cycle_debug_frame_overlay_mode();
            debug_frame_overlay_mode_name(window.debug_frame_overlay_mode()).to_string()
        });

        #[cfg(any(target_os = "windows", target_os = "linux", target_os = "freebsd"))]
        {
            let (response, receiver) = sync_channel(1);
            self.send_ui_command(UiCommand::CycleDebugFrameOverlay { response })?;
            return recv_ui_response(receiver, "the debug frame overlay query");
        }

        #[cfg(not(any(
            target_os = "macos",
            target_os = "windows",
            target_os = "linux",
            target_os = "freebsd"
        )))]
        Err(Error::from_reason("Unsupported operating system"))
    }

    #[napi]
    pub fn get_debug_frame_overlay(&self) -> Result<String> {
        self.debug_frame_overlay_mode()
    }

    fn debug_frame_overlay_mode(&self) -> Result<String> {
        #[cfg(target_os = "macos")]
        return update_window(|_view, window, _cx| {
            debug_frame_overlay_mode_name(window.debug_frame_overlay_mode()).to_string()
        });

        #[cfg(any(target_os = "windows", target_os = "linux", target_os = "freebsd"))]
        {
            let (response, receiver) = sync_channel(1);
            self.send_ui_command(UiCommand::GetDebugFrameOverlay { response })?;
            recv_ui_response(receiver, "the debug frame overlay query")
        }

        #[cfg(not(any(
            target_os = "macos",
            target_os = "windows",
            target_os = "linux",
            target_os = "freebsd"
        )))]
        Err(Error::from_reason("Unsupported operating system"))
    }

    /// Clears the last 1000 draw samples. Frame count stays.
    #[napi]
    pub fn reset_debug_frame_overlay_stats(&self) -> Result<()> {
        #[cfg(target_os = "macos")]
        return update_window(|_view, window, _cx| {
            window.reset_debug_frame_overlay_stats();
        });

        #[cfg(any(target_os = "windows", target_os = "linux", target_os = "freebsd"))]
        return self.send_ui_command(UiCommand::ResetDebugFrameOverlayStats);

        #[cfg(not(any(
            target_os = "macos",
            target_os = "windows",
            target_os = "linux",
            target_os = "freebsd"
        )))]
        Err(Error::from_reason("Unsupported operating system"))
    }

    /// Same numbers as the on-screen overlay: current, p90, p99, max, frames.
    #[napi]
    pub fn get_debug_frame_overlay_stats(&self) -> Result<DebugFrameOverlayStats> {
        #[cfg(target_os = "macos")]
        return update_window(|_view, window, _cx| {
            debug_frame_overlay_stats_js(window.debug_frame_overlay_stats())
        });

        #[cfg(any(target_os = "windows", target_os = "linux", target_os = "freebsd"))]
        {
            let (response, receiver) = sync_channel(1);
            self.send_ui_command(UiCommand::GetDebugFrameOverlayStats { response })?;
            match receiver.recv_timeout(Duration::from_secs(2)) {
                Ok(stats) => Ok(stats),
                Err(RecvTimeoutError::Timeout) => Err(Error::from_reason(
                    "Timed out after 2 seconds waiting for debug frame overlay stats",
                )),
                Err(RecvTimeoutError::Disconnected) => Err(Error::from_reason(
                    "The GPUI UI thread stopped during the debug frame overlay stats query",
                )),
            }
        }

        #[cfg(not(any(
            target_os = "macos",
            target_os = "windows",
            target_os = "linux",
            target_os = "freebsd"
        )))]
        Err(Error::from_reason("Unsupported operating system"))
    }

    #[napi]
    pub fn set_window_title(&self, title: String) -> Result<()> {
        #[cfg(target_os = "macos")]
        return update_window(move |view, window, cx| {
            view.window_title = title;
            cx.notify();
            window.refresh();
        });

        #[cfg(any(target_os = "windows", target_os = "linux", target_os = "freebsd"))]
        return self.send_ui_command(UiCommand::SetWindowTitle(title));

        #[cfg(not(any(
            target_os = "macos",
            target_os = "windows",
            target_os = "linux",
            target_os = "freebsd"
        )))]
        Err(Error::from_reason(
            "The production GPUIX renderer does not support this operating system",
        ))
    }

    #[napi]
    pub fn focus_element(&self, element_id: f64) -> Result<()> {
        let id = to_element_id(element_id)?;
        #[cfg(target_os = "macos")]
        return update_window(move |view, window, cx| {
            view.reveal_virtual_list_ancestor(id);
            if let Some(handle) = view.focus_handles.get(&id) {
                handle.focus(window, cx);
            }
            cx.notify();
            window.refresh();
        });

        #[cfg(any(target_os = "windows", target_os = "linux", target_os = "freebsd"))]
        return self.send_ui_command(UiCommand::FocusElement(id));

        #[cfg(not(any(
            target_os = "macos",
            target_os = "windows",
            target_os = "linux",
            target_os = "freebsd"
        )))]
        Err(Error::from_reason("Unsupported operating system"))
    }

    #[napi]
    pub fn blur(&self) -> Result<()> {
        #[cfg(target_os = "macos")]
        return update_window(move |_view, window, _cx| window.blur());

        #[cfg(any(target_os = "windows", target_os = "linux", target_os = "freebsd"))]
        return self.send_ui_command(UiCommand::Blur);

        #[cfg(not(any(
            target_os = "macos",
            target_os = "windows",
            target_os = "linux",
            target_os = "freebsd"
        )))]
        Err(Error::from_reason("Unsupported operating system"))
    }

    // ── Selection API ────────────────────────────────────────────────

    /// The current text selection joined in document order, or null.
    #[napi]
    pub fn get_selected_text(&self) -> Option<String> {
        self.selection.lock().selected_text()
    }

    /// Drop the current selection and request a repaint.
    #[napi]
    pub fn clear_selection(&self) -> Result<()> {
        self.selection.lock().clear();
        self.request_invalidate()
    }

    // ── Scroll API ───────────────────────────────────────────────────
    // GpuixView syncs scroll handles and virtual list states to thread-local maps.

    /// Set the scroll offset of a scrollable element.
    /// x and y are negative pixel values (scroll down = more negative y).
    #[napi]
    pub fn scroll_to(&self, element_id: f64, x: f64, y: f64) -> Result<()> {
        let id = to_element_id(element_id)?;
        #[cfg(target_os = "macos")]
        if !VIRTUAL_LIST_STATES.with(|cell| {
            let states = cell.borrow();
            let Some(state) = states.get(&id) else {
                return false;
            };
            state.set_offset_from_scrollbar(gpui::point(gpui::px(x as f32), gpui::px(y as f32)));
            true
        }) {
            SCROLL_HANDLES.with(|cell| {
                let handles = cell.borrow();
                if let Some(handle) = handles.get(&id) {
                    handle.set_offset(gpui::point(gpui::px(x as f32), gpui::px(y as f32)));
                }
            });
        }
        #[cfg(target_os = "macos")]
        return invalidate_window();

        #[cfg(any(target_os = "windows", target_os = "linux", target_os = "freebsd"))]
        return self.send_ui_command(UiCommand::ScrollTo {
            id,
            x: x as f32,
            y: y as f32,
        });

        #[cfg(not(any(
            target_os = "macos",
            target_os = "windows",
            target_os = "linux",
            target_os = "freebsd"
        )))]
        Err(Error::from_reason("Unsupported operating system"))
    }

    /// Scroll a child into view by its index in the children list.
    #[napi]
    pub fn scroll_to_item(&self, element_id: f64, index: f64) -> Result<()> {
        let id = to_element_id(element_id)?;
        let index = index as usize;
        #[cfg(target_os = "macos")]
        if !VIRTUAL_LIST_STATES.with(|cell| {
            let states = cell.borrow();
            let Some(state) = states.get(&id) else {
                return false;
            };
            state.scroll_to(gpui::ListOffset {
                item_ix: index,
                offset_in_item: gpui::px(0.0),
            });
            true
        }) {
            SCROLL_HANDLES.with(|cell| {
                let handles = cell.borrow();
                if let Some(handle) = handles.get(&id) {
                    handle.scroll_to_item(index);
                }
            });
        }
        #[cfg(target_os = "macos")]
        return invalidate_window();

        #[cfg(any(target_os = "windows", target_os = "linux", target_os = "freebsd"))]
        return self.send_ui_command(UiCommand::ScrollToItem { id, index });

        #[cfg(not(any(
            target_os = "macos",
            target_os = "windows",
            target_os = "linux",
            target_os = "freebsd"
        )))]
        Err(Error::from_reason("Unsupported operating system"))
    }

    /// Get the current scroll offset of a scrollable element.
    /// Returns [x, y] or null if the element has no scroll handle.
    #[napi]
    pub fn get_scroll_offset(&self, element_id: f64) -> Result<Option<Vec<f64>>> {
        let id = to_element_id(element_id)?;
        #[cfg(target_os = "macos")]
        return Ok(VIRTUAL_LIST_STATES
            .with(|cell| {
                cell.borrow().get(&id).map(|state| {
                    let offset = state.scroll_px_offset_for_scrollbar();
                    vec![
                        f64::from(f32::from(offset.x)),
                        f64::from(f32::from(offset.y)),
                    ]
                })
            })
            .or_else(|| {
                SCROLL_HANDLES.with(|cell| {
                    let handles = cell.borrow();
                    handles.get(&id).map(|handle| {
                        let offset = handle.offset();
                        vec![
                            f64::from(f32::from(offset.x)),
                            f64::from(f32::from(offset.y)),
                        ]
                    })
                })
            }));

        #[cfg(any(target_os = "windows", target_os = "linux", target_os = "freebsd"))]
        {
            let (response, receiver) = sync_channel(1);
            self.send_ui_command(UiCommand::GetScrollOffset { id, response })?;
            return Ok(
                recv_ui_response(receiver, "the GPUI scroll query")?.map(|[x, y]| vec![x, y])
            );
        }

        #[cfg(not(any(
            target_os = "macos",
            target_os = "windows",
            target_os = "linux",
            target_os = "freebsd"
        )))]
        Err(Error::from_reason("Unsupported operating system"))
    }

    #[napi]
    pub fn get_automation_tree(&self) -> Result<String> {
        self.request_invalidate()?;
        let bounds = self.automation_bounds()?;
        let tree = self.tree.lock().unwrap();
        let json = tree.to_automation_json(&bounds);
        serde_json::to_string(&json)
            .map_err(|e| Error::from_reason(format!("JSON serialization failed: {}", e)))
    }

    #[napi]
    pub fn get_element_bounds(&self, id: f64) -> Result<Option<Vec<f64>>> {
        let id = to_element_id(id)?;
        Ok(self
            .element_bounds(id)?
            .map(|bounds| vec![bounds.x, bounds.y, bounds.width, bounds.height]))
    }

    #[napi]
    pub fn get_all_text(&self) -> Vec<String> {
        let tree = self.tree.lock().unwrap();
        let mut texts = Vec::new();
        if let Some(root_id) = tree.root_id {
            collect_text(root_id, &tree, &mut texts);
        }
        texts
    }

    #[napi]
    pub fn get_painted_text(&self) -> Vec<String> {
        crate::text::painted_text()
    }

    /// Every highlight wash painted in the last frame, in paint order.
    ///
    /// A quad is invisible to `getPaintedText()`, so this is the only way to
    /// assert on `highlight` without a screenshot.
    #[napi]
    pub fn get_painted_highlights(&self) -> Vec<crate::element_tree::HighlightMatch> {
        crate::text::painted_highlights()
            .into_iter()
            .map(Into::into)
            .collect()
    }

    /// `modifiers` uses the `press()` syntax: "cmd", "cmd-shift", "alt".
    #[napi]
    pub fn simulate_click(
        &self,
        x: f64,
        y: f64,
        button: Option<u32>,
        modifiers: Option<String>,
    ) -> Result<()> {
        let button = button.unwrap_or(0);
        let modifiers = crate::automation::parse_modifiers(modifiers.as_deref());

        #[cfg(target_os = "macos")]
        return update_window(move |_view, window, cx| {
            crate::automation::dispatch_click(window, cx, x, y, button, modifiers);
        });

        #[cfg(any(target_os = "windows", target_os = "linux", target_os = "freebsd"))]
        return self.dispatch_mouse_input(MouseInput::Click {
            x,
            y,
            button,
            modifiers,
        });

        #[cfg(not(any(
            target_os = "macos",
            target_os = "windows",
            target_os = "linux",
            target_os = "freebsd"
        )))]
        {
            let _ = (x, y, button);
            Err(Error::from_reason(
                "The production GPUIX renderer does not support this operating system",
            ))
        }
    }

    #[napi]
    pub fn simulate_mouse_down(
        &self,
        x: f64,
        y: f64,
        button: Option<u32>,
        modifiers: Option<String>,
    ) -> Result<()> {
        let button = button.unwrap_or(0);
        let modifiers = crate::automation::parse_modifiers(modifiers.as_deref());

        #[cfg(target_os = "macos")]
        return update_window(move |_view, window, cx| {
            crate::automation::dispatch_mouse_down(window, cx, x, y, button, modifiers);
        });

        #[cfg(any(target_os = "windows", target_os = "linux", target_os = "freebsd"))]
        return self.dispatch_mouse_input(MouseInput::Down {
            x,
            y,
            button,
            modifiers,
        });

        #[cfg(not(any(
            target_os = "macos",
            target_os = "windows",
            target_os = "linux",
            target_os = "freebsd"
        )))]
        {
            let _ = (x, y, button);
            Err(Error::from_reason(
                "The production GPUIX renderer does not support this operating system",
            ))
        }
    }

    #[napi]
    pub fn simulate_mouse_up(
        &self,
        x: f64,
        y: f64,
        button: Option<u32>,
        modifiers: Option<String>,
    ) -> Result<()> {
        let button = button.unwrap_or(0);
        let modifiers = crate::automation::parse_modifiers(modifiers.as_deref());

        #[cfg(target_os = "macos")]
        return update_window(move |_view, window, cx| {
            crate::automation::dispatch_mouse_up(window, cx, x, y, button, modifiers);
        });

        #[cfg(any(target_os = "windows", target_os = "linux", target_os = "freebsd"))]
        return self.dispatch_mouse_input(MouseInput::Up {
            x,
            y,
            button,
            modifiers,
        });

        #[cfg(not(any(
            target_os = "macos",
            target_os = "windows",
            target_os = "linux",
            target_os = "freebsd"
        )))]
        {
            let _ = (x, y, button);
            Err(Error::from_reason(
                "The production GPUIX renderer does not support this operating system",
            ))
        }
    }

    #[napi]
    pub fn simulate_mouse_move(
        &self,
        x: f64,
        y: f64,
        pressed_button: Option<u32>,
        modifiers: Option<String>,
    ) -> Result<()> {
        let modifiers = crate::automation::parse_modifiers(modifiers.as_deref());

        #[cfg(target_os = "macos")]
        return update_window(move |_view, window, cx| {
            crate::automation::dispatch_mouse_move(window, cx, x, y, pressed_button, modifiers);
        });

        #[cfg(any(target_os = "windows", target_os = "linux", target_os = "freebsd"))]
        return self.dispatch_mouse_input(MouseInput::Move {
            x,
            y,
            pressed_button,
            modifiers,
        });

        #[cfg(not(any(
            target_os = "macos",
            target_os = "windows",
            target_os = "linux",
            target_os = "freebsd"
        )))]
        {
            let _ = (x, y, pressed_button);
            Err(Error::from_reason(
                "The production GPUIX renderer does not support this operating system",
            ))
        }
    }

    /// Dispatch a wheel event through the same GPUI hit test the trackpad uses.
    /// Deltas are pixels: negative `delta_y` scrolls down, negative `delta_x`
    /// pans right, matching `TestGpuixRenderer::simulate_scroll_wheel`.
    #[napi]
    pub fn simulate_scroll_wheel(
        &self,
        x: f64,
        y: f64,
        delta_x: f64,
        delta_y: f64,
        modifiers: Option<String>,
    ) -> Result<()> {
        let modifiers = crate::automation::parse_modifiers(modifiers.as_deref());

        #[cfg(target_os = "macos")]
        return update_window(move |_view, window, cx| {
            crate::automation::dispatch_scroll_wheel(
                window, cx, x, y, delta_x, delta_y, modifiers,
            );
        });

        #[cfg(any(target_os = "windows", target_os = "linux", target_os = "freebsd"))]
        return self.dispatch_mouse_input(MouseInput::Wheel {
            x,
            y,
            delta_x,
            delta_y,
            modifiers,
        });

        #[cfg(not(any(
            target_os = "macos",
            target_os = "windows",
            target_os = "linux",
            target_os = "freebsd"
        )))]
        {
            let _ = (x, y, delta_x, delta_y, modifiers);
            Err(Error::from_reason(
                "The production GPUIX renderer does not support this operating system",
            ))
        }
    }

    #[napi]
    pub fn clock_pause(&self) -> Result<f64> {
        #[cfg(target_os = "macos")]
        return update_window(move |view, _window, cx| {
            let now_ms = view.clock.pause();
            cx.notify();
            now_ms
        });

        #[cfg(any(target_os = "windows", target_os = "linux", target_os = "freebsd"))]
        return self.control_clock(ClockControl::Pause);

        #[cfg(not(any(
            target_os = "macos",
            target_os = "windows",
            target_os = "linux",
            target_os = "freebsd"
        )))]
        Err(Error::from_reason("Unsupported operating system"))
    }

    #[napi]
    pub fn clock_set(&self, now_ms: f64) -> Result<f64> {
        #[cfg(target_os = "macos")]
        return update_window(move |view, _window, cx| {
            let now_ms = view.clock.set_ms(now_ms);
            cx.notify();
            now_ms
        });

        #[cfg(any(target_os = "windows", target_os = "linux", target_os = "freebsd"))]
        return self.control_clock(ClockControl::Set(now_ms));

        #[cfg(not(any(
            target_os = "macos",
            target_os = "windows",
            target_os = "linux",
            target_os = "freebsd"
        )))]
        {
            let _ = now_ms;
            Err(Error::from_reason("Unsupported operating system"))
        }
    }

    #[napi]
    pub fn clock_fast_forward(&self, delta_ms: f64) -> Result<f64> {
        #[cfg(target_os = "macos")]
        return update_window(move |view, _window, cx| {
            let now_ms = view.clock.fast_forward_ms(delta_ms);
            cx.notify();
            now_ms
        });

        #[cfg(any(target_os = "windows", target_os = "linux", target_os = "freebsd"))]
        return self.control_clock(ClockControl::FastForward(delta_ms));

        #[cfg(not(any(
            target_os = "macos",
            target_os = "windows",
            target_os = "linux",
            target_os = "freebsd"
        )))]
        {
            let _ = delta_ms;
            Err(Error::from_reason("Unsupported operating system"))
        }
    }

    #[napi]
    pub fn clock_resume(&self) -> Result<f64> {
        #[cfg(target_os = "macos")]
        return update_window(move |view, _window, cx| {
            let now_ms = view.clock.resume();
            cx.notify();
            now_ms
        });

        #[cfg(any(target_os = "windows", target_os = "linux", target_os = "freebsd"))]
        return self.control_clock(ClockControl::Resume);

        #[cfg(not(any(
            target_os = "macos",
            target_os = "windows",
            target_os = "linux",
            target_os = "freebsd"
        )))]
        Err(Error::from_reason("Unsupported operating system"))
    }

    #[napi]
    pub fn capture_screenshot(&self, path: String) -> Result<()> {
        #[cfg(all(target_os = "macos", feature = "test-support"))]
        {
            let image = update_window(move |_view, window, cx| {
                cx.notify();
                window.refresh();
                window.render_to_image()
            })?
            .map_err(|e| Error::from_reason(format!("Screenshot capture failed: {}", e)))?;
            image
                .save(&path)
                .map_err(|e| Error::from_reason(format!("Failed to save screenshot: {}", e)))?;
            Ok(())
        }

        #[cfg(all(target_os = "windows", feature = "test-support"))]
        {
            let (response, receiver) = sync_channel(1);
            self.send_ui_command(UiCommand::CaptureScreenshot { path, response })?;
            return recv_ui_response(receiver, "screenshot capture")?.map_err(Error::from_reason);
        }

        #[cfg(not(all(
            feature = "test-support",
            any(target_os = "macos", target_os = "windows")
        )))]
        {
            let _ = path;
            Err(Error::from_reason(
                "captureScreenshot needs a test-support build on macOS or Windows",
            ))
        }
    }
}

fn collect_text(id: u64, tree: &RetainedTree, texts: &mut Vec<String>) {
    if let Some(element) = tree.elements.get(&id) {
        if let Some(ref content) = element.content {
            texts.push(content.clone());
        }
        for &child_id in &element.children {
            collect_text(child_id, tree, texts);
        }
    }
}

#[cfg(all(target_arch = "wasm32", target_os = "unknown"))]
fn start_web_app(
    tree: Arc<Mutex<RetainedTree>>,
    selection: SharedSelection,
    event_callback: EventCallback,
) -> Result<(), wasm_bindgen::JsValue> {
    if WEB_APP.with(|stored| stored.borrow().is_some()) {
        return Err(wasm_bindgen::JsValue::from_str(
            "GPUIX web is already running",
        ));
    }
    gpui_platform::web_init();
    let app = gpui_platform::single_threaded_web().run_embedded(move |cx| {
        init_key_bindings(cx);
        crate::custom_elements::input::init(cx);
        let window = cx.open_window(Default::default(), |window, cx| {
            if let Some(mode) = PENDING_DEBUG_OVERLAY.with(|pending| pending.borrow_mut().take()) {
                window.set_debug_frame_overlay_mode(mode);
            }
            cx.new(|_| {
                GpuixView::new(
                    tree,
                    Some(event_callback),
                    "GPUIX Web".to_string(),
                    selection,
                )
            })
        });
        match window {
            Ok(window) => WEB_WINDOW.with(|stored| *stored.borrow_mut() = Some(window)),
            Err(error) => log::error!("Failed to open the GPUIX web window: {error:#}"),
        }
        cx.activate(true);
    });
    WEB_APP.with(|stored| *stored.borrow_mut() = Some(app));
    Ok(())
}

#[cfg(all(target_arch = "wasm32", target_os = "unknown"))]
fn web_element_id(id: f64) -> Result<u64, wasm_bindgen::JsValue> {
    raw_element_id(id).map_err(|error| wasm_bindgen::JsValue::from_str(&error))
}

#[cfg(all(target_arch = "wasm32", target_os = "unknown"))]
fn web_number_array(values: impl IntoIterator<Item = f64>) -> wasm_bindgen::JsValue {
    let result = js_sys::Array::new();
    for value in values {
        result.push(&value.into());
    }
    result.into()
}

#[cfg(all(target_arch = "wasm32", target_os = "unknown"))]
fn web_string_array(values: impl IntoIterator<Item = String>) -> wasm_bindgen::JsValue {
    let result = js_sys::Array::new();
    for value in values {
        result.push(&value.into());
    }
    result.into()
}

#[cfg(all(target_arch = "wasm32", target_os = "unknown"))]
fn update_web_window<R>(
    update: impl FnOnce(&mut GpuixView, &mut gpui::Window, &mut gpui::Context<GpuixView>) -> R,
) -> Result<R, wasm_bindgen::JsValue> {
    WEB_APP.with(|app| {
        let app = app.borrow();
        let app = app
            .as_ref()
            .ok_or_else(|| wasm_bindgen::JsValue::from_str("GPUIX web is not initialized"))?;
        app.update(|cx| {
            WEB_WINDOW.with(|window| {
                let window = (*window.borrow()).ok_or_else(|| {
                    wasm_bindgen::JsValue::from_str("GPUIX web window is not ready")
                })?;
                window
                    .update(cx, update)
                    .map_err(|error| wasm_bindgen::JsValue::from_str(&error.to_string()))
            })
        })
    })
}

#[cfg(all(target_arch = "wasm32", target_os = "unknown"))]
fn notify_web() {
    if let Err(error) = update_web_window(|_view, _window, cx| cx.notify()) {
        if WEB_WINDOW.with(|window| window.borrow().is_some()) {
            log::error!("Failed to invalidate the GPUIX web window: {error:?}");
        }
    }
}

#[cfg(all(target_arch = "wasm32", target_os = "unknown"))]
fn web_event_callback(callback: js_sys::Function) -> EventCallback {
    Rc::new(move |payload| {
        let Ok(json) = serde_json::to_string(&payload) else {
            log::error!("Failed to serialize GPUIX browser event");
            return;
        };
        let Ok(payload) = js_sys::JSON::parse(&json) else {
            log::error!("Failed to create GPUIX browser event object");
            return;
        };
        let callback = callback.clone();
        let task = wasm_bindgen::closure::Closure::once_into_js(move || {
            if let Err(error) = callback.call2(
                &wasm_bindgen::JsValue::UNDEFINED,
                &wasm_bindgen::JsValue::NULL,
                &payload,
            ) {
                log::error!("GPUIX browser event callback failed: {error:?}");
            }
        });
        let task: js_sys::Function = task.unchecked_into();
        if let Some(window) = web_sys::window() {
            window.queue_microtask(&task);
        }
    })
}

#[cfg(all(target_arch = "wasm32", target_os = "unknown"))]
#[wasm_bindgen::prelude::wasm_bindgen(js_name = GpuixRenderer)]
pub struct WebGpuixRenderer {
    tree: Arc<Mutex<RetainedTree>>,
    selection: SharedSelection,
    event_callback: EventCallback,
}

#[cfg(all(target_arch = "wasm32", target_os = "unknown"))]
#[wasm_bindgen::prelude::wasm_bindgen(js_class = GpuixRenderer)]
impl WebGpuixRenderer {
    #[wasm_bindgen::prelude::wasm_bindgen(constructor)]
    pub fn new(event_callback: js_sys::Function) -> Self {
        Self {
            tree: Arc::new(Mutex::new(RetainedTree::new())),
            selection: SharedSelection::default(),
            event_callback: web_event_callback(event_callback),
        }
    }

    pub fn init(&self, _options: wasm_bindgen::JsValue) -> Result<(), wasm_bindgen::JsValue> {
        start_web_app(
            self.tree.clone(),
            self.selection.clone(),
            self.event_callback.clone(),
        )
    }

    #[wasm_bindgen::prelude::wasm_bindgen(js_name = createElement)]
    pub fn create_element(
        &self,
        id: f64,
        element_type: String,
    ) -> Result<(), wasm_bindgen::JsValue> {
        self.tree
            .lock()
            .unwrap()
            .create_element(web_element_id(id)?, element_type);
        Ok(())
    }

    #[wasm_bindgen::prelude::wasm_bindgen(js_name = destroyElement)]
    pub fn destroy_element(&self, id: f64) -> Result<wasm_bindgen::JsValue, wasm_bindgen::JsValue> {
        let destroyed = self
            .tree
            .lock()
            .unwrap()
            .destroy_element(web_element_id(id)?)
            .into_iter()
            .map(|id| id as f64);
        Ok(web_number_array(destroyed))
    }

    #[wasm_bindgen::prelude::wasm_bindgen(js_name = appendChild)]
    pub fn append_child(&self, parent_id: f64, child_id: f64) -> Result<(), wasm_bindgen::JsValue> {
        self.tree
            .lock()
            .unwrap()
            .append_child(web_element_id(parent_id)?, web_element_id(child_id)?);
        Ok(())
    }

    #[wasm_bindgen::prelude::wasm_bindgen(js_name = removeChild)]
    pub fn remove_child(&self, parent_id: f64, child_id: f64) -> Result<(), wasm_bindgen::JsValue> {
        self.tree
            .lock()
            .unwrap()
            .remove_child(web_element_id(parent_id)?, web_element_id(child_id)?);
        Ok(())
    }

    #[wasm_bindgen::prelude::wasm_bindgen(js_name = insertBefore)]
    pub fn insert_before(
        &self,
        parent_id: f64,
        child_id: f64,
        before_id: f64,
    ) -> Result<(), wasm_bindgen::JsValue> {
        self.tree.lock().unwrap().insert_before(
            web_element_id(parent_id)?,
            web_element_id(child_id)?,
            web_element_id(before_id)?,
        );
        Ok(())
    }

    #[wasm_bindgen::prelude::wasm_bindgen(js_name = setStyle)]
    pub fn set_style(&self, id: f64, style_json: String) -> Result<(), wasm_bindgen::JsValue> {
        let id = web_element_id(id)?;
        self.tree
            .lock()
            .unwrap()
            .set_style_json(id, style_json.as_bytes())
            .map_err(|error| {
                wasm_bindgen::JsValue::from_str(&format!("Failed to parse style: {error}"))
            })
    }

    #[wasm_bindgen::prelude::wasm_bindgen(js_name = setText)]
    pub fn set_text(&self, id: f64, content: String) -> Result<(), wasm_bindgen::JsValue> {
        self.tree
            .lock()
            .unwrap()
            .set_text(web_element_id(id)?, content);
        Ok(())
    }

    #[wasm_bindgen::prelude::wasm_bindgen(js_name = setEventListener)]
    pub fn set_event_listener(
        &self,
        id: f64,
        event_type: String,
        has_handler: bool,
    ) -> Result<(), wasm_bindgen::JsValue> {
        self.tree
            .lock()
            .unwrap()
            .set_event_listener(web_element_id(id)?, event_type, has_handler);
        Ok(())
    }

    #[wasm_bindgen::prelude::wasm_bindgen(js_name = setRoot)]
    pub fn set_root(&self, id: f64) -> Result<(), wasm_bindgen::JsValue> {
        self.tree.lock().unwrap().root_id = Some(web_element_id(id)?);
        Ok(())
    }

    #[wasm_bindgen::prelude::wasm_bindgen(js_name = setCustomProp)]
    pub fn set_custom_prop(
        &self,
        id: f64,
        key: String,
        value_json: String,
    ) -> Result<(), wasm_bindgen::JsValue> {
        let value = serde_json::from_str(&value_json).map_err(|error| {
            wasm_bindgen::JsValue::from_str(&format!("Failed to parse custom prop: {error}"))
        })?;
        self.tree
            .lock()
            .unwrap()
            .set_custom_prop(web_element_id(id)?, key, value);
        Ok(())
    }

    #[wasm_bindgen::prelude::wasm_bindgen(js_name = getCustomProp)]
    pub fn get_custom_prop(
        &self,
        id: f64,
        key: String,
    ) -> Result<wasm_bindgen::JsValue, wasm_bindgen::JsValue> {
        let value = self
            .tree
            .lock()
            .unwrap()
            .get_custom_prop(web_element_id(id)?, &key)
            .map(serde_json::Value::to_string);
        Ok(value.map_or(wasm_bindgen::JsValue::NULL, |value| {
            wasm_bindgen::JsValue::from_str(&value)
        }))
    }

    #[wasm_bindgen::prelude::wasm_bindgen(js_name = applyBatch)]
    pub fn apply_batch(
        &self,
        json: String,
    ) -> Result<wasm_bindgen::JsValue, wasm_bindgen::JsValue> {
        let destroyed = apply_batch_to_tree(&mut self.tree.lock().unwrap(), json.as_bytes())
            .map_err(|error| wasm_bindgen::JsValue::from_str(&error))?;
        notify_web();
        Ok(web_number_array(destroyed))
    }

    #[wasm_bindgen::prelude::wasm_bindgen(js_name = commitMutations)]
    pub fn commit_mutations(&self) {
        notify_web();
    }

    #[wasm_bindgen::prelude::wasm_bindgen(js_name = isInitialized)]
    pub fn is_initialized(&self) -> bool {
        WEB_APP.with(|app| app.borrow().is_some())
    }

    #[wasm_bindgen::prelude::wasm_bindgen(js_name = requiresTick)]
    pub fn requires_tick(&self) -> bool {
        false
    }

    pub fn tick(&self) {}

    #[wasm_bindgen::prelude::wasm_bindgen(js_name = getWindowSize)]
    pub fn get_window_size(&self) -> Result<wasm_bindgen::JsValue, wasm_bindgen::JsValue> {
        let window = web_sys::window()
            .ok_or_else(|| wasm_bindgen::JsValue::from_str("Browser window is unavailable"))?;
        let size = js_sys::Object::new();
        js_sys::Reflect::set(&size, &"width".into(), &window.inner_width()?)?;
        js_sys::Reflect::set(&size, &"height".into(), &window.inner_height()?)?;
        Ok(size.into())
    }

    #[wasm_bindgen::prelude::wasm_bindgen(js_name = getWindowInsets)]
    pub fn get_window_insets(&self) -> Result<wasm_bindgen::JsValue, wasm_bindgen::JsValue> {
        let insets = update_web_window(|_view, window, _cx| window.insets())?;
        window_insets_js(insets)
    }

    #[wasm_bindgen::prelude::wasm_bindgen(js_name = setWindowTitle)]
    pub fn set_window_title(&self, title: String) -> Result<(), wasm_bindgen::JsValue> {
        update_web_window(move |view, _window, cx| {
            view.window_title = title;
            cx.notify();
        })
    }

    #[wasm_bindgen::prelude::wasm_bindgen(js_name = focusElement)]
    pub fn focus_element(&self, element_id: f64) -> Result<(), wasm_bindgen::JsValue> {
        let id = web_element_id(element_id)?;
        update_web_window(move |view, window, cx| {
            view.reveal_virtual_list_ancestor(id);
            if let Some(handle) = view.focus_handles.get(&id) {
                handle.focus(window, cx);
            }
            cx.notify();
        })
    }

    pub fn blur(&self) -> Result<(), wasm_bindgen::JsValue> {
        update_web_window(|_view, window, _cx| window.blur())
    }

    #[wasm_bindgen::prelude::wasm_bindgen(js_name = getSelectedText)]
    pub fn get_selected_text(&self) -> wasm_bindgen::JsValue {
        self.selection
            .lock()
            .selected_text()
            .map_or(wasm_bindgen::JsValue::NULL, |value| {
                wasm_bindgen::JsValue::from_str(&value)
            })
    }

    #[wasm_bindgen::prelude::wasm_bindgen(js_name = clearSelection)]
    pub fn clear_selection(&self) {
        self.selection.lock().clear();
        notify_web();
    }

    #[wasm_bindgen::prelude::wasm_bindgen(js_name = scrollTo)]
    pub fn scroll_to(&self, element_id: f64, x: f64, y: f64) -> Result<(), wasm_bindgen::JsValue> {
        let id = web_element_id(element_id)?;
        if !VIRTUAL_LIST_STATES.with(|states| {
            let states = states.borrow();
            let Some(state) = states.get(&id) else {
                return false;
            };
            state.set_offset_from_scrollbar(gpui::point(gpui::px(x as f32), gpui::px(y as f32)));
            true
        }) {
            SCROLL_HANDLES.with(|handles| {
                if let Some(handle) = handles.borrow().get(&id) {
                    handle.set_offset(gpui::point(gpui::px(x as f32), gpui::px(y as f32)));
                }
            });
        }
        notify_web();
        Ok(())
    }

    #[wasm_bindgen::prelude::wasm_bindgen(js_name = scrollToItem)]
    pub fn scroll_to_item(&self, element_id: f64, index: f64) -> Result<(), wasm_bindgen::JsValue> {
        let id = web_element_id(element_id)?;
        let index = index as usize;
        if !VIRTUAL_LIST_STATES.with(|states| {
            let states = states.borrow();
            let Some(state) = states.get(&id) else {
                return false;
            };
            state.scroll_to(gpui::ListOffset {
                item_ix: index,
                offset_in_item: gpui::px(0.0),
            });
            true
        }) {
            SCROLL_HANDLES.with(|handles| {
                if let Some(handle) = handles.borrow().get(&id) {
                    handle.scroll_to_item(index);
                }
            });
        }
        notify_web();
        Ok(())
    }

    #[wasm_bindgen::prelude::wasm_bindgen(js_name = getScrollOffset)]
    pub fn get_scroll_offset(
        &self,
        element_id: f64,
    ) -> Result<wasm_bindgen::JsValue, wasm_bindgen::JsValue> {
        let id = web_element_id(element_id)?;
        let offset = VIRTUAL_LIST_STATES
            .with(|states| {
                states.borrow().get(&id).map(|state| {
                    let offset = state.scroll_px_offset_for_scrollbar();
                    [
                        f64::from(f32::from(offset.x)),
                        f64::from(f32::from(offset.y)),
                    ]
                })
            })
            .or_else(|| {
                SCROLL_HANDLES.with(|handles| {
                    handles.borrow().get(&id).map(|handle| {
                        let offset = handle.offset();
                        [
                            f64::from(f32::from(offset.x)),
                            f64::from(f32::from(offset.y)),
                        ]
                    })
                })
            });
        let Some([x, y]) = offset else {
            return Ok(wasm_bindgen::JsValue::NULL);
        };
        Ok(web_number_array([x, y]))
    }

    #[wasm_bindgen::prelude::wasm_bindgen(js_name = getAutomationTree)]
    pub fn get_automation_tree(&self) -> Result<String, wasm_bindgen::JsValue> {
        notify_web();
        let bounds = crate::automation::all_bounds();
        let tree = self.tree.lock().unwrap();
        serde_json::to_string(&tree.to_automation_json(&bounds)).map_err(|error| {
            wasm_bindgen::JsValue::from_str(&format!("JSON serialization failed: {error}"))
        })
    }

    #[wasm_bindgen::prelude::wasm_bindgen(js_name = getElementBounds)]
    pub fn get_element_bounds(
        &self,
        element_id: f64,
    ) -> Result<wasm_bindgen::JsValue, wasm_bindgen::JsValue> {
        let Some(bounds) = crate::automation::get_bounds(web_element_id(element_id)?) else {
            return Ok(wasm_bindgen::JsValue::NULL);
        };
        Ok(web_number_array([
            bounds.x,
            bounds.y,
            bounds.width,
            bounds.height,
        ]))
    }

    #[wasm_bindgen::prelude::wasm_bindgen(js_name = getAllText)]
    pub fn get_all_text(&self) -> wasm_bindgen::JsValue {
        let tree = self.tree.lock().unwrap();
        let mut texts = Vec::new();
        if let Some(root_id) = tree.root_id {
            collect_text(root_id, &tree, &mut texts);
        }
        web_string_array(texts)
    }

    #[wasm_bindgen::prelude::wasm_bindgen(js_name = getPaintedText)]
    pub fn get_painted_text(&self) -> wasm_bindgen::JsValue {
        web_string_array(crate::text::painted_text())
    }

    /// The same array of objects the napi build returns.
    ///
    /// Through `serde_json` and `JSON.parse`, not `serde-wasm-bindgen`: this is
    /// a test-only API, and both crates here are already dependencies. Building
    /// the nested value by hand with `js_sys` is 20 lines of noise.
    #[wasm_bindgen::prelude::wasm_bindgen(js_name = getPaintedHighlights)]
    pub fn get_painted_highlights(&self) -> wasm_bindgen::JsValue {
        let matches: Vec<crate::element_tree::HighlightMatch> = crate::text::painted_highlights()
            .into_iter()
            .map(Into::into)
            .collect();
        serde_json::to_string(&matches)
            .ok()
            .and_then(|json| js_sys::JSON::parse(&json).ok())
            .unwrap_or(wasm_bindgen::JsValue::NULL)
    }

    #[wasm_bindgen::prelude::wasm_bindgen(js_name = simulateClick)]
    pub fn simulate_click(
        &self,
        x: f64,
        y: f64,
        button: Option<u32>,
        modifiers: Option<String>,
    ) -> Result<(), wasm_bindgen::JsValue> {
        let modifiers = crate::automation::parse_modifiers(modifiers.as_deref());
        update_web_window(move |_view, window, cx| {
            crate::automation::dispatch_click(window, cx, x, y, button.unwrap_or(0), modifiers);
            cx.notify();
        })
    }

    #[wasm_bindgen::prelude::wasm_bindgen(js_name = simulateMouseDown)]
    pub fn simulate_mouse_down(
        &self,
        x: f64,
        y: f64,
        button: Option<u32>,
        modifiers: Option<String>,
    ) -> Result<(), wasm_bindgen::JsValue> {
        let modifiers = crate::automation::parse_modifiers(modifiers.as_deref());
        update_web_window(move |_view, window, cx| {
            crate::automation::dispatch_mouse_down(window, cx, x, y, button.unwrap_or(0), modifiers);
            cx.notify();
        })
    }

    #[wasm_bindgen::prelude::wasm_bindgen(js_name = simulateMouseUp)]
    pub fn simulate_mouse_up(
        &self,
        x: f64,
        y: f64,
        button: Option<u32>,
        modifiers: Option<String>,
    ) -> Result<(), wasm_bindgen::JsValue> {
        let modifiers = crate::automation::parse_modifiers(modifiers.as_deref());
        update_web_window(move |_view, window, cx| {
            crate::automation::dispatch_mouse_up(window, cx, x, y, button.unwrap_or(0), modifiers);
            cx.notify();
        })
    }

    #[wasm_bindgen::prelude::wasm_bindgen(js_name = simulateMouseMove)]
    pub fn simulate_mouse_move(
        &self,
        x: f64,
        y: f64,
        pressed_button: Option<u32>,
        modifiers: Option<String>,
    ) -> Result<(), wasm_bindgen::JsValue> {
        let modifiers = crate::automation::parse_modifiers(modifiers.as_deref());
        update_web_window(move |_view, window, cx| {
            crate::automation::dispatch_mouse_move(window, cx, x, y, pressed_button, modifiers);
            cx.notify();
        })
    }

    #[wasm_bindgen::prelude::wasm_bindgen(js_name = simulateScrollWheel)]
    pub fn simulate_scroll_wheel(
        &self,
        x: f64,
        y: f64,
        delta_x: f64,
        delta_y: f64,
        modifiers: Option<String>,
    ) -> Result<(), wasm_bindgen::JsValue> {
        let modifiers = crate::automation::parse_modifiers(modifiers.as_deref());
        update_web_window(move |_view, window, cx| {
            crate::automation::dispatch_scroll_wheel(
                window, cx, x, y, delta_x, delta_y, modifiers,
            );
            cx.notify();
        })
    }

    #[wasm_bindgen::prelude::wasm_bindgen(js_name = clockPause)]
    pub fn clock_pause(&self) -> Result<f64, wasm_bindgen::JsValue> {
        update_web_window(|view, _window, cx| {
            let now_ms = view.clock.pause();
            cx.notify();
            now_ms
        })
    }

    #[wasm_bindgen::prelude::wasm_bindgen(js_name = clockSet)]
    pub fn clock_set(&self, now_ms: f64) -> Result<f64, wasm_bindgen::JsValue> {
        update_web_window(move |view, _window, cx| {
            let now_ms = view.clock.set_ms(now_ms);
            cx.notify();
            now_ms
        })
    }

    #[wasm_bindgen::prelude::wasm_bindgen(js_name = clockFastForward)]
    pub fn clock_fast_forward(&self, delta_ms: f64) -> Result<f64, wasm_bindgen::JsValue> {
        update_web_window(move |view, _window, cx| {
            let now_ms = view.clock.fast_forward_ms(delta_ms);
            cx.notify();
            now_ms
        })
    }

    #[wasm_bindgen::prelude::wasm_bindgen(js_name = clockResume)]
    pub fn clock_resume(&self) -> Result<f64, wasm_bindgen::JsValue> {
        update_web_window(|view, _window, cx| {
            let now_ms = view.clock.resume();
            cx.notify();
            now_ms
        })
    }

    #[wasm_bindgen::prelude::wasm_bindgen(js_name = setDebugFrameOverlay)]
    pub fn set_debug_frame_overlay(&self, mode: String) -> Result<String, wasm_bindgen::JsValue> {
        let mode = parse_debug_frame_overlay_mode_str(&mode)
            .map_err(|error| wasm_bindgen::JsValue::from_str(&error))?;
        // Graphics init is async. render() sets the overlay before WEB_WINDOW exists.
        if WEB_WINDOW.with(|window| window.borrow().is_none()) {
            PENDING_DEBUG_OVERLAY.with(|pending| *pending.borrow_mut() = Some(mode));
            return Ok(debug_frame_overlay_mode_name(mode).to_string());
        }
        update_web_window(move |_view, window, cx| {
            window.set_debug_frame_overlay_mode(mode);
            cx.notify();
            debug_frame_overlay_mode_name(window.debug_frame_overlay_mode()).to_string()
        })
    }

    #[wasm_bindgen::prelude::wasm_bindgen(js_name = getDebugFrameOverlay)]
    pub fn get_debug_frame_overlay(&self) -> Result<String, wasm_bindgen::JsValue> {
        if WEB_WINDOW.with(|window| window.borrow().is_none()) {
            let pending = PENDING_DEBUG_OVERLAY.with(|pending| *pending.borrow());
            return Ok(debug_frame_overlay_mode_name(
                pending.unwrap_or(gpui::DebugFrameOverlayMode::Hidden),
            )
            .to_string());
        }
        update_web_window(|_view, window, _cx| {
            debug_frame_overlay_mode_name(window.debug_frame_overlay_mode()).to_string()
        })
    }
}

#[cfg(any(target_os = "windows", target_os = "linux", target_os = "freebsd"))]
impl Drop for GpuixRenderer {
    fn drop(&mut self) {
        self.ui_commands.lock().unwrap().take();
    }
}

// ── GPUI View ────────────────────────────────────────────────────────

pub(crate) struct GpuixView {
    pub(crate) tree: Arc<Mutex<RetainedTree>>,
    pub(crate) event_callback: Option<EventCallback>,
    pub(crate) window_title: String,
    /// Persistent FocusHandles keyed by element ID.
    /// Created lazily for elements with keyboard or focus/blur listeners.
    /// Handles persist across renders so GPUI maintains focus state.
    pub(crate) focus_handles: HashMap<u64, gpui::FocusHandle>,
    /// Active focus/blur subscriptions keyed by element and event type.
    pub(crate) focus_subscriptions: HashMap<(u64, String), gpui::Subscription>,
    /// Registry for custom element types (input, editor, diff, etc.).
    /// Stores factories (one per type) and live instances (one per element ID).
    pub(crate) custom_registry: CustomElementRegistry,
    /// Persistent ScrollHandles keyed by element ID.
    /// Created lazily for elements with overflow: "scroll" (or per-axis scroll).
    /// Handles persist across renders so GPUI maintains scroll offset state.
    pub(crate) scroll_handles: HashMap<u64, gpui::ScrollHandle>,
    /// Native animation clocks keyed by retained element ID.
    pub(crate) motion_states: HashMap<u64, crate::motion::MotionState>,
    /// Live text selection, shared with the paint closures and the napi methods.
    pub(crate) selection: SharedSelection,
    /// Persistent measurement and scroll state for React-backed virtual lists.
    virtual_lists: HashMap<u64, VirtualListEntry>,
    /// Motion / review clock. Live wall time unless automation freezes it.
    pub(crate) clock: crate::automation::AutomationClock,
    /// The cascade every frame starts from.
    ///
    /// It has to keep its identity between frames. The resolved-style cache
    /// asks whether the cascade an element resolved under is still the current
    /// one, and it compares pointers. A fresh root on every frame would answer
    /// no for every element that reads an inherited value, and the whole tree
    /// would resolve again on every frame.
    root_cascade: RefCell<Option<(Theme, gpui::Pixels, crate::inheritance::Inherited)>>,
    /// Resolved `highlight` state, keyed by the element that declared it.
    /// Empty in every app that does not use search.
    highlights: HashMap<u64, HighlightCacheEntry>,
}

/// Two-level cache for one element's `highlight`.
///
/// The group list is keyed by `search_revision`, which a query change does NOT
/// move, so typing in a find bar never re-walks or re-folds text. The matches
/// are additionally keyed by the matcher hash, which excludes `activeIndex` and
/// the colours, so moving the find cursor only re-colours what it already found.
///
/// Do not key the group list on `subtree_revision`: `highlight` is a custom
/// prop, so every keystroke moves that revision and the cache would do nothing.
/// `highlight_cache_tests` at the bottom of this file compares `Arc` identity
/// and fails if either level regresses. A timing budget does not catch it: on
/// the 1000-turn chat the broken version is 2.7ms against 1.9ms.
struct HighlightCacheEntry {
    revision: u64,
    groups: Arc<crate::text::GroupList>,
    matcher_hash: u64,
    /// The spec plus the located matches. Ordinals and colours are decided at
    /// paint, so a colour or `activeIndex` change reuses this whole value.
    context: Arc<crate::text::HighlightContext>,
    /// Last identity delivered through `onHighlight`. Only written once an
    /// event is really queued, so adding the listener later still reports.
    reported: Option<u64>,
}

fn emit_highlight_events(callback: &Option<EventCallback>, events: &[(u64, usize)]) {
    for &(id, total) in events {
        emit_event_full(callback, id, "highlight", |payload| {
            payload.match_count = Some(total as f64);
        });
    }
}

/// Resolve one element's `highlight` prop, reusing both cache levels.
///
/// Returns the context, plus the match count when `has_listener` and the result
/// differs from the last one this element reported. Identity, not count:
/// swapping a query for a different one with the same number of hits is still a
/// new result.
fn resolve_highlight(
    cache: &mut HashMap<u64, HighlightCacheEntry>,
    tree: &RetainedTree,
    id: u64,
    value: &serde_json::Value,
    theme: &Theme,
    has_listener: bool,
) -> Option<(Arc<crate::text::HighlightContext>, Option<usize>)> {
    let set = crate::text::HighlightSet::parse(value, theme)?;
    // `search_revision`, NOT `subtree_revision`: `highlight` is a custom prop,
    // so the general revision moves on every keystroke and this cache would
    // never hit for the one case it exists for.
    let revision = tree.elements.get(&id)?.search_revision;
    let matcher_hash = set.matcher_hash();

    let cached = cache
        .get(&id)
        .filter(|entry| entry.revision == revision && entry.matcher_hash == matcher_hash);
    let context = match cached {
        // Nothing moved at all. Returning the same `Arc` keeps the whole
        // subtree's inherited value identical, which the cache tests assert.
        Some(entry) if entry.context.set == set => entry.context.clone(),
        // Same matches, different colours or find cursor: reuse the located
        // matches and swap only the spec. No text is scanned.
        Some(entry) => {
            let context = Arc::new(crate::text::HighlightContext {
                declaration: id,
                set,
                matches: entry.context.matches.clone(),
            });
            cache.get_mut(&id)?.context = context.clone();
            context
        }
        None => {
            let groups = match cache.get(&id) {
                Some(entry) if entry.revision == revision => entry.groups.clone(),
                _ => Arc::new(crate::text::GroupList::collect(tree, id)),
            };
            let context = Arc::new(crate::text::HighlightContext {
                declaration: id,
                matches: Arc::new(crate::text::search::resolve(&groups, &set)),
                set,
            });
            let reported = cache.get(&id).and_then(|entry| entry.reported);
            cache.insert(
                id,
                HighlightCacheEntry {
                    revision,
                    groups,
                    matcher_hash,
                    context: context.clone(),
                    reported,
                },
            );
            context
        }
    };

    if !has_listener {
        return Some((context, None));
    }
    let identity = context.matches.identity();
    let entry = cache.get_mut(&id)?;
    if entry.reported == Some(identity) {
        return Some((context, None));
    }
    entry.reported = Some(identity);
    let total = context.matches.total;
    Some((context, Some(total)))
}

impl GpuixView {
    pub(crate) fn new(
        tree: Arc<Mutex<RetainedTree>>,
        event_callback: Option<EventCallback>,
        window_title: String,
        selection: SharedSelection,
    ) -> Self {
        Self {
            tree,
            event_callback,
            window_title,
            focus_handles: HashMap::new(),
            focus_subscriptions: HashMap::new(),
            custom_registry: CustomElementRegistry::with_defaults(),
            scroll_handles: HashMap::new(),
            motion_states: HashMap::new(),
            selection,
            virtual_lists: HashMap::new(),
            clock: crate::automation::AutomationClock::new(),
            root_cascade: RefCell::new(None),
            highlights: HashMap::new(),
        }
    }

    /// The root cascade for `theme`, reusing the last one while the theme
    /// holds still.
    fn root_cascade(&self, theme: &Theme, rem_size: gpui::Pixels) -> crate::inheritance::Inherited {
        let mut slot = self.root_cascade.borrow_mut();
        if let Some((built_for, built_rem, cascade)) = slot.as_ref() {
            if built_for == theme && *built_rem == rem_size {
                return cascade.clone();
            }
        }
        // Inheritance takes plain values, so the theme is read here rather
        // than inside it.
        let cascade = crate::inheritance::Inherited::root(
            crate::color::from_gpui(theme.accent),
            theme.dark,
            f32::from(rem_size),
        );
        *slot = Some((theme.clone(), rem_size, cascade.clone()));
        cascade
    }

    fn build_virtual_child(
        &mut self,
        list_id: u64,
        index: usize,
        expected_child_id: u64,
        cascade: crate::inheritance::Inherited,
        highlight: Option<std::sync::Arc<crate::text::HighlightContext>>,
        window: &mut gpui::Window,
        cx: &mut gpui::Context<Self>,
    ) -> gpui::AnyElement {
        use gpui::prelude::*;

        let row_focus_handle = self.virtual_lists.get_mut(&list_id).and_then(|entry| {
            entry.seen_rows.insert(expected_child_id);
            (entry.child_at(index) == Some(expected_child_id))
                .then(|| {
                    index
                        .checked_sub(entry.window_start)
                        .and_then(|offset| entry.row_focus_handles.get(offset).cloned())
                })
                .flatten()
                .flatten()
        });

        let tree_arc = self.tree.clone();
        let tree = tree_arc.lock().unwrap();
        let window_start = self
            .virtual_lists
            .get(&list_id)
            .map(|entry| entry.window_start)
            .unwrap_or(0);
        let child_matches = tree.elements.get(&list_id).and_then(|list| {
            index
                .checked_sub(window_start)
                .and_then(|offset| list.children.get(offset))
        }) == Some(&expected_child_id);
        if !child_matches {
            let height = self
                .virtual_lists
                .get(&list_id)
                .and_then(|entry| entry.config.estimated_item_height)
                .unwrap_or(1.0);
            return unmounted_virtual_row(height);
        }

        let callback = self.event_callback.clone();
        let now = self.clock.now();
        let mut motion_active = false;
        let mut highlight_events = Vec::new();

        // Re-resolve against the tree as it is NOW. gpui calls this during
        // layout and prepaint, after the root render returned, and on Windows
        // and Linux the Node thread can commit new text in between. Reusing the
        // captured ranges would paint a wash over the wrong glyphs, or at a byte
        // offset that is no longer a character boundary.
        let mut highlight = highlight;
        if let Some(declaration) = highlight.as_ref().map(|ctx| ctx.declaration) {
            highlight = tree
                .elements
                .get(&declaration)
                .and_then(|element| element.custom_props.get("highlight"))
                .and_then(|value| {
                    resolve_highlight(
                        &mut self.highlights,
                        &tree,
                        declaration,
                        value,
                        &Theme::dark(),
                        false,
                    )
                })
                .map(|(context, _)| context);
        }

        let mut build_ctx = BuildCtx {
            tree: &tree,
            event_callback: &callback,
            focus_handles: &self.focus_handles,
            scroll_handles: &mut self.scroll_handles,
            custom_registry: &mut self.custom_registry,
            virtual_lists: &mut self.virtual_lists,
            motion_states: &mut self.motion_states,
            now,
            motion_active: &mut motion_active,
            selection: self.selection.clone(),
            cascade,
            highlight,
            highlights: &mut self.highlights,
            highlight_events: &mut highlight_events,
            direct_rules: Vec::new(),
            descendant_rules: Vec::new(),
        };
        // A virtual row builds outside the tree walk, so it has no child
        // position and the index states do not apply to it.
        let child = build_element(expected_child_id, None, &mut build_ctx, window, cx);
        emit_highlight_events(&callback, &highlight_events);
        if motion_active {
            window.request_animation_frame();
        }
        let Some(focus_handle) = row_focus_handle else {
            return child;
        };
        gpui::div()
            .id(gpui::SharedString::from(format!(
                "__gpuix_virtual_row_{}_{}",
                list_id, expected_child_id
            )))
            .w_full()
            .track_focus(&focus_handle)
            .child(child)
            .into_any_element()
    }

    pub(crate) fn scroll_virtual_list_to_item(&self, id: u64, index: usize) -> bool {
        let Some(entry) = self.virtual_lists.get(&id) else {
            return false;
        };
        entry.state.scroll_to(gpui::ListOffset {
            item_ix: index,
            offset_in_item: gpui::px(0.0),
        });
        emit_event_full(&self.event_callback, id, "visibleRange", |payload| {
            payload.start_index = Some(index as f64);
            payload.end_index = Some((index + 1) as f64);
        });
        true
    }

    pub(crate) fn set_virtual_list_offset(&self, id: u64, x: f32, y: f32) -> bool {
        let Some(entry) = self.virtual_lists.get(&id) else {
            return false;
        };
        entry
            .state
            .set_offset_from_scrollbar(gpui::point(gpui::px(x), gpui::px(y)));
        true
    }

    pub(crate) fn virtual_list_offset(&self, id: u64) -> Option<[f64; 2]> {
        let offset = self
            .virtual_lists
            .get(&id)?
            .state
            .scroll_px_offset_for_scrollbar();
        Some([
            f64::from(f32::from(offset.x)),
            f64::from(f32::from(offset.y)),
        ])
    }

    pub(crate) fn reveal_virtual_list_ancestor(&self, id: u64) -> bool {
        let tree_arc = self.tree.clone();
        let tree = tree_arc.lock().unwrap();
        let mut current = id;
        let location = loop {
            let Some(parent_id) = tree
                .elements
                .get(&current)
                .and_then(|element| element.parent)
            else {
                break None;
            };
            if self.virtual_lists.contains_key(&parent_id) {
                let index = tree
                    .elements
                    .get(&parent_id)
                    .and_then(|parent| parent.children.iter().position(|child| *child == current));
                break index.map(|index| (parent_id, index));
            }
            current = parent_id;
        };
        drop(tree);

        let Some((list_id, index)) = location else {
            return false;
        };
        self.scroll_virtual_list_to_item(list_id, index)
    }
}


impl GpuixView {
    /// Sync focus handles with the current element tree.
    /// Creates handles for new focusable elements, subscribes on_focus/on_blur,
    /// and cleans up handles for destroyed elements.
    fn sync_focus_handles(
        &mut self,
        tree: &RetainedTree,
        callback: &Option<EventCallback>,
        window: &mut gpui::Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let tab_index = |element: &crate::retained_tree::RetainedElement| {
            element
                .custom_props
                .get("tabIndex")
                .and_then(|value| value.as_i64())
                .and_then(|index| isize::try_from(index).ok())
        };
        let needs_focus = |element: &crate::retained_tree::RetainedElement| {
            matches!(element.element_type.as_str(), "input" | "textarea")
                || tab_index(element).is_some()
                || element.events.contains("keyDown")
                || element.events.contains("keyUp")
                || element.events.contains("focus")
                || element.events.contains("blur")
        };
        // Create handles for elements that need focus but don't have one yet.
        for (&id, element) in &tree.elements {
            let tab_index = tab_index(element).or_else(|| {
                matches!(element.element_type.as_str(), "input" | "textarea").then_some(0)
            });

            if needs_focus(element) && !self.focus_handles.contains_key(&id) {
                let handle = match tab_index {
                    Some(index) => cx.focus_handle().tab_index(index).tab_stop(index >= 0),
                    None => cx.focus_handle(),
                };
                // Focus once, at creation. Re-focusing every frame would
                // steal focus back from whatever the user clicked next.
                if element.auto_focus {
                    handle.focus(window, cx);
                }
                self.focus_handles.insert(id, handle);
            } else if let (Some(handle), Some(index)) =
                (self.focus_handles.get(&id).cloned(), tab_index)
            {
                self.focus_handles
                    .insert(id, handle.tab_index(index).tab_stop(index >= 0));
            } else if let Some(handle) = self.focus_handles.get(&id).cloned() {
                self.focus_handles.insert(id, handle.tab_stop(false));
            }
        }

        self.focus_subscriptions.retain(|(id, event), _| {
            tree.elements
                .get(id)
                .is_some_and(|element| element.events.contains(event))
        });
        for (&id, element) in &tree.elements {
            let Some(handle) = self.focus_handles.get(&id).cloned() else {
                continue;
            };
            let focus_key = (id, "focus".to_string());
            if element.events.contains("focus")
                && !self.focus_subscriptions.contains_key(&focus_key)
            {
                let callback = callback.clone();
                let subscription = cx.on_focus(&handle, window, move |_this, _window, _cx| {
                    emit_event_full(&callback, id, "focus", |_| {});
                });
                self.focus_subscriptions.insert(focus_key, subscription);
            }
            let blur_key = (id, "blur".to_string());
            if element.events.contains("blur") && !self.focus_subscriptions.contains_key(&blur_key)
            {
                let callback = callback.clone();
                let subscription = cx.on_blur(&handle, window, move |_this, _window, _cx| {
                    emit_event_full(&callback, id, "blur", |_| {});
                });
                self.focus_subscriptions.insert(blur_key, subscription);
            }
        }

        // Clean up handles for elements that no longer exist.
        self.focus_handles
            .retain(|id, _| tree.elements.get(id).is_some_and(&needs_focus));
    }
}

impl gpui::Render for GpuixView {
    fn render(
        &mut self,
        window: &mut gpui::Window,
        cx: &mut gpui::Context<Self>,
    ) -> impl gpui::IntoElement {
        use gpui::IntoElement;

        window.set_window_title(&self.window_title);

        // Clone Arc so we don't borrow self.tree — frees self for focus_handles access.
        let tree_arc = self.tree.clone();
        let tree = tree_arc.lock().unwrap();
        let callback = self.event_callback.clone();

        // Sync focus handles before building elements.
        self.sync_focus_handles(&tree, &callback, window, cx);

        // Ensure custom element instances are destroyed when their IDs disappear.
        self.custom_registry
            .prune_missing(|id| tree.elements.contains_key(&id));

        // Clean up scroll handles for destroyed elements (IDs removed from tree).
        // Scrollability-based cleanup (element still exists but style changed
        // from scroll to non-scroll) is handled inside build_div().
        self.scroll_handles
            .retain(|id, _| tree.elements.contains_key(id));
        self.virtual_lists
            .retain(|id, _| tree.elements.contains_key(id));
        self.motion_states
            .retain(|id, _| tree.elements.contains_key(id));

        // Build the element tree. custom_registry, focus_handles, and scroll_handles
        // are different fields of self, so Rust allows borrowing all simultaneously.
        let theme = Theme::dark();
        let root_cascade = self.root_cascade(&theme, window.rem_size());
        let now = self.clock.now();
        let mut motion_active = false;
        // Pruned by DECLARATION, not existence: an element that drops its
        // `highlight` prop keeps living, and its cached group list holds a copy
        // of every string in its subtree.
        self.highlights.retain(|id, _| {
            tree.elements
                .get(id)
                .is_some_and(|element| element.custom_props.contains_key("highlight"))
        });
        let mut highlight_events = Vec::new();
        let result = match tree.root_id {
            Some(root_id) => {
                let mut ctx = BuildCtx {
                    tree: &tree,
                    event_callback: &callback,
                    focus_handles: &self.focus_handles,
                    scroll_handles: &mut self.scroll_handles,
                    custom_registry: &mut self.custom_registry,
                    virtual_lists: &mut self.virtual_lists,
                    motion_states: &mut self.motion_states,
                    now,
                    motion_active: &mut motion_active,
                    selection: self.selection.clone(),
                    cascade: root_cascade,
                    highlight: None,
                    highlights: &mut self.highlights,
                    highlight_events: &mut highlight_events,
                    direct_rules: Vec::new(),
                    descendant_rules: Vec::new(),
                };
                build_element(root_id, None, &mut ctx, window, cx)
            }
            None => gpui::Empty.into_any_element(),
        };
        // Flushed after the root build so a `setState` in the handler cannot
        // re-enter this build.
        emit_highlight_events(&callback, &highlight_events);

        // The frame reset must paint BEFORE any text, so it is the first child of
        // the root wrapper. Without it the selection registry accumulates stale
        // entries across frames and a drag resolves against elements that are no
        // longer on screen.
        let result = {
            use gpui::prelude::*;
            let root = gpui::div()
                .size_full()
                .on_action(|_: &FocusNext, window, cx| window.focus_next(cx))
                .on_action(|_: &FocusPrevious, window, cx| window.focus_prev(cx));
            with_window_menu_actions(root)
                .child(selection_frame_reset(self.selection.clone()))
                .child(crate::automation::bounds_frame_reset())
                .child(result)
                .into_any_element()
        };

        // Sync scroll handles to thread_local so napi methods (scrollTo,
        // getScrollOffset) can access them without an App context.
        SCROLL_HANDLES.with(|cell| {
            let mut handles = cell.borrow_mut();
            handles.clear();
            for (&id, handle) in &self.scroll_handles {
                handles.insert(id, handle.clone());
            }
        });
        VIRTUAL_LIST_STATES.with(|cell| {
            let mut states = cell.borrow_mut();
            states.clear();
            for (&id, entry) in &self.virtual_lists {
                states.insert(id, entry.state.clone());
            }
        });

        if motion_active {
            window.request_animation_frame();
        }

        result
    }
}


// ── Event emission ───────────────────────────────────────────────────

/// Helper to convert a GPUI Point<Pixels> to (f64, f64).
pub(crate) fn point_to_xy(p: gpui::Point<gpui::Pixels>) -> (f64, f64) {
    (f64::from(f32::from(p.x)), f64::from(f32::from(p.y)))
}

/// Convert GPUI MouseButton to our u32 encoding: 0=left, 1=middle, 2=right.
pub(crate) fn mouse_button_to_u32(button: gpui::MouseButton) -> u32 {
    match button {
        gpui::MouseButton::Left => 0,
        gpui::MouseButton::Middle => 1,
        gpui::MouseButton::Right => 2,
        gpui::MouseButton::Navigate(_) => 3,
    }
}

/// General-purpose event emitter. Builds a default EventPayload, lets the
/// caller customize it via a closure, then sends it through the callback.
/// Production: queues on Node.js event loop via ThreadsafeFunction.
/// Tests: pushes to a synchronous Vec for drainEvents().
pub(crate) fn emit_event_full(
    callback: &Option<EventCallback>,
    element_id: u64,
    event_type: &str,
    build: impl FnOnce(&mut EventPayload),
) {
    if let Some(cb) = callback {
        let mut payload = EventPayload {
            element_id: element_id as f64,
            event_type: event_type.to_string(),
            ..Default::default()
        };
        build(&mut payload);
        cb(payload);
    }
}


// ── Types ────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
#[cfg_attr(not(all(target_arch = "wasm32", target_os = "unknown")), napi(object))]
pub struct WindowSize {
    pub width: f64,
    pub height: f64,
}

#[derive(Debug, Clone, Default)]
#[cfg_attr(not(all(target_arch = "wasm32", target_os = "unknown")), napi(object))]
pub struct EdgeInsets {
    pub top: f64,
    pub right: f64,
    pub bottom: f64,
    pub left: f64,
}

#[derive(Debug, Clone, Default)]
#[cfg_attr(not(all(target_arch = "wasm32", target_os = "unknown")), napi(object))]
pub struct WindowInsets {
    pub safe_area: EdgeInsets,
    pub ime: EdgeInsets,
    pub effective: EdgeInsets,
}

impl WindowInsets {
    fn from_gpui(insets: gpui::WindowInsets) -> Self {
        let effective = insets.effective();
        Self {
            safe_area: EdgeInsets::from_gpui(insets.safe_area),
            ime: EdgeInsets::from_gpui(insets.ime),
            effective: EdgeInsets::from_gpui(effective),
        }
    }
}

impl EdgeInsets {
    fn from_gpui(insets: gpui::Edges<gpui::Pixels>) -> Self {
        Self {
            top: f32::from(insets.top) as f64,
            right: f32::from(insets.right) as f64,
            bottom: f32::from(insets.bottom) as f64,
            left: f32::from(insets.left) as f64,
        }
    }
}

#[cfg(all(target_arch = "wasm32", target_os = "unknown"))]
fn edge_insets_js(
    insets: gpui::Edges<gpui::Pixels>,
) -> Result<wasm_bindgen::JsValue, wasm_bindgen::JsValue> {
    let object = js_sys::Object::new();
    for (key, value) in [
        ("top", insets.top),
        ("right", insets.right),
        ("bottom", insets.bottom),
        ("left", insets.left),
    ] {
        js_sys::Reflect::set(
            &object,
            &wasm_bindgen::JsValue::from_str(key),
            &wasm_bindgen::JsValue::from_f64(f32::from(value) as f64),
        )?;
    }
    Ok(object.into())
}

#[cfg(all(target_arch = "wasm32", target_os = "unknown"))]
fn window_insets_js(
    insets: gpui::WindowInsets,
) -> Result<wasm_bindgen::JsValue, wasm_bindgen::JsValue> {
    let effective = insets.effective();
    let object = js_sys::Object::new();
    for (key, value) in [
        ("safeArea", edge_insets_js(insets.safe_area)?),
        ("ime", edge_insets_js(insets.ime)?),
        ("effective", edge_insets_js(effective)?),
    ] {
        js_sys::Reflect::set(&object, &wasm_bindgen::JsValue::from_str(key), &value)?;
    }
    Ok(object.into())
}

/// Recorded draw times from the debug frame overlay.
#[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
#[derive(Debug, Clone)]
#[napi(object)]
pub struct DebugFrameOverlayStats {
    pub current_ms: Option<f64>,
    pub p90_ms: Option<f64>,
    pub p99_ms: Option<f64>,
    pub max_ms: Option<f64>,
    pub frames: f64,
    pub samples: f64,
}

#[derive(Debug, Clone)]
#[cfg_attr(not(all(target_arch = "wasm32", target_os = "unknown")), napi(object))]
pub struct WindowOptions {
    pub title: Option<String>,
    /// The name used inside the macOS "Hide" and "Quit" menu items. Defaults to
    /// `title`. It does NOT set the title of the application menu itself: macOS
    /// takes that from the executable, and only a `.app` bundle changes it.
    pub app_name: Option<String>,
    pub width: Option<f64>,
    pub height: Option<f64>,
    pub min_width: Option<f64>,
    pub min_height: Option<f64>,
    pub resizable: Option<bool>,
    pub fullscreen: Option<bool>,
    /// Plain alpha transparency. Prefer `window_background` when you need blur.
    pub transparent: Option<bool>,
    /// Hide the native titlebar so the app can draw chrome under the traffic lights.
    pub titlebar_transparent: Option<bool>,
    /// `"opaque"` | `"transparent"` | `"blurred"`. `transparent: true` is the
    /// same as `"transparent"` when this is unset.
    pub window_background: Option<String>,
    pub traffic_light_x: Option<f64>,
    pub traffic_light_y: Option<f64>,
}

impl Default for WindowOptions {
    fn default() -> Self {
        Self {
            title: Some("GPUIX".to_string()),
            app_name: None,
            width: Some(800.0),
            height: Some(600.0),
            min_width: None,
            min_height: None,
            resizable: Some(true),
            fullscreen: Some(false),
            transparent: Some(false),
            titlebar_transparent: Some(false),
            window_background: None,
            traffic_light_x: None,
            traffic_light_y: None,
        }
    }
}

fn to_gpui_window_options(
    options: &WindowOptions,
    bounds: gpui::Bounds<gpui::Pixels>,
) -> gpui::WindowOptions {
    let title = options.title.clone().unwrap_or_else(|| "GPUIX".to_string());
    let titlebar_transparent = options.titlebar_transparent.unwrap_or(false);
    let traffic_light_position = match (options.traffic_light_x, options.traffic_light_y) {
        (Some(x), Some(y)) => Some(gpui::point(gpui::px(x as f32), gpui::px(y as f32))),
        _ => None,
    };
    let window_background = match options.window_background.as_deref() {
        Some("transparent") => gpui::WindowBackgroundAppearance::Transparent,
        Some("blurred") => gpui::WindowBackgroundAppearance::Blurred,
        Some("opaque") => gpui::WindowBackgroundAppearance::Opaque,
        _ if options.transparent.unwrap_or(false) => gpui::WindowBackgroundAppearance::Transparent,
        _ => gpui::WindowBackgroundAppearance::Opaque,
    };
    let window_min_size = match (options.min_width, options.min_height) {
        (Some(width), Some(height)) => {
            Some(gpui::size(gpui::px(width as f32), gpui::px(height as f32)))
        }
        _ => None,
    };
    let window_bounds = if options.fullscreen.unwrap_or(false) {
        gpui::WindowBounds::Fullscreen(bounds)
    } else {
        gpui::WindowBounds::Windowed(bounds)
    };
    gpui::WindowOptions {
        window_bounds: Some(window_bounds),
        titlebar: Some(gpui::TitlebarOptions {
            title: Some(title.into()),
            appears_transparent: titlebar_transparent,
            traffic_light_position,
        }),
        is_resizable: options.resizable.unwrap_or(true),
        window_background,
        window_min_size,
        ..Default::default()
    }
}

#[cfg(test)]
mod highlight_cache_tests {
    use super::*;

    fn tree_with_text() -> RetainedTree {
        let mut tree = RetainedTree::new();
        tree.create_element(1, "div".to_string());
        tree.create_element(2, "text".to_string());
        tree.append_child(1, 2);
        tree.set_text(2, "a fox and a fox".to_string());
        tree
    }

    fn query(text: &str) -> serde_json::Value {
        serde_json::json!({ "query": text })
    }

    fn declare(tree: &mut RetainedTree, value: &serde_json::Value) {
        tree.set_custom_prop(1, "highlight".to_string(), value.clone());
    }

    /// The whole reason `search_revision` exists. `highlight` is a custom prop,
    /// so keying the group list on `subtree_revision` means every keystroke
    /// re-walks and re-folds the subtree. The pointer comparison is the proof;
    /// a timing budget over a realistic app is far too coarse to catch it.
    #[test]
    fn a_query_change_reuses_the_group_list() {
        let theme = Theme::dark();
        let mut tree = tree_with_text();
        let mut cache = HashMap::new();

        declare(&mut tree, &query("f"));
        resolve_highlight(&mut cache, &tree, 1, &query("f"), &theme, false).expect("resolves");
        let first = Arc::as_ptr(&cache[&1].groups);

        declare(&mut tree, &query("fo"));
        resolve_highlight(&mut cache, &tree, 1, &query("fo"), &theme, false).expect("resolves");
        assert_eq!(
            Arc::as_ptr(&cache[&1].groups),
            first,
            "a query change must not rebuild the group list"
        );
    }

    /// Moving a find cursor changes no text and no matcher, so it must re-use
    /// the located matches. Colours and ordinals are decided at paint.
    #[test]
    fn a_cursor_move_reuses_the_located_matches() {
        let theme = Theme::dark();
        let mut tree = tree_with_text();
        let mut cache = HashMap::new();
        let spec = |active: u64| serde_json::json!({ "query": "fox", "activeIndex": active });

        declare(&mut tree, &spec(0));
        resolve_highlight(&mut cache, &tree, 1, &spec(0), &theme, true).expect("resolves");
        let matches = Arc::as_ptr(&cache[&1].context.matches);

        declare(&mut tree, &spec(1));
        let (context, changed) =
            resolve_highlight(&mut cache, &tree, 1, &spec(1), &theme, true).expect("resolves");
        assert_eq!(Arc::as_ptr(&context.matches), matches, "no rescan");
        assert_eq!(changed, None, "a cursor move is not a new result");
        assert_eq!(context.set.specs[0].active_index, Some(1), "spec still swapped");
    }

    /// Editing the text must invalidate, or the wash paints over stale offsets.
    #[test]
    fn a_text_change_rebuilds_the_group_list() {
        let theme = Theme::dark();
        let mut tree = tree_with_text();
        let mut cache = HashMap::new();

        declare(&mut tree, &query("fox"));
        resolve_highlight(&mut cache, &tree, 1, &query("fox"), &theme, true).expect("resolves");
        let first = Arc::as_ptr(&cache[&1].groups);

        tree.set_text(2, "one fox only".to_string());
        let (_, changed) =
            resolve_highlight(&mut cache, &tree, 1, &query("fox"), &theme, true).expect("resolves");
        assert_ne!(Arc::as_ptr(&cache[&1].groups), first);
        assert_eq!(changed, Some(1), "two matches became one");
    }

    /// A review caught this: `reported` used to be written even with no
    /// listener, so mounting without `onHighlight` and adding it later reported
    /// nothing, forever.
    #[test]
    fn adding_the_listener_later_still_reports() {
        let theme = Theme::dark();
        let mut tree = tree_with_text();
        let mut cache = HashMap::new();

        declare(&mut tree, &query("fox"));
        let (_, changed) =
            resolve_highlight(&mut cache, &tree, 1, &query("fox"), &theme, false).expect("resolves");
        assert_eq!(changed, None, "nothing to report without a listener");

        let (_, changed) =
            resolve_highlight(&mut cache, &tree, 1, &query("fox"), &theme, true).expect("resolves");
        assert_eq!(changed, Some(2), "the listener gets the current count");

        let (_, changed) =
            resolve_highlight(&mut cache, &tree, 1, &query("fox"), &theme, true).expect("resolves");
        assert_eq!(changed, None, "and only once");
    }
}

/// The `applyBatch` protocol. This is the surface JS talks to, so every rule it
/// relies on is asserted here against real JSON bytes rather than through a
/// hand-built `Vec<BatchOp>`.
#[cfg(test)]
mod batch_tests {
    use super::*;
    use crate::retained_tree::STYLE_SWEEP_FLOOR;

    fn apply(tree: &mut RetainedTree, json: &str) -> batch::BatchResult<Vec<f64>> {
        apply_batch_to_tree(tree, json.as_bytes())
    }

    /// Everything a mutation can reach, so an unwanted partial apply shows up
    /// as a diff instead of hiding in a field the test forgot to read.
    fn describe(tree: &RetainedTree) -> String {
        let mut ids: Vec<_> = tree.elements.keys().copied().collect();
        ids.sort_unstable();
        let mut out = format!("root={:?}\n", tree.root_id);
        for id in ids {
            let element = &tree.elements[&id];
            let mut events: Vec<_> = element.events.iter().cloned().collect();
            events.sort();
            let mut props: Vec<_> = element.custom_props.iter().collect();
            props.sort_by(|(a, _), (b, _)| a.cmp(b));
            out += &format!(
                "{id} type={} text={:?} style={:?} children={:?} parent={:?} events={events:?} props={props:?} rev={}/{}\n",
                element.element_type,
                element.content,
                element.style.as_deref(),
                element.children,
                element.parent,
                element.subtree_revision,
                element.search_revision,
            );
        }
        out
    }

    /// The regression test for batch atomicity. `intern_style_payload` used to
    /// run inside the apply loop, so this batch created the element, set its
    /// text, and only then threw — leaving JS to retry against a tree that had
    /// already moved.
    #[test]
    fn a_malformed_style_applies_nothing_at_all() {
        let mut tree = RetainedTree::new();
        apply(&mut tree, r#"[["createElement",1,"div"],["setRoot",1]]"#).expect("valid batch");
        let before = describe(&tree);
        let styles_before = tree.styles.len();

        let error = apply(
            &mut tree,
            r#"[["createElement",2,"div"],["setText",2,"changed"],["setStyle",2,123]]"#,
        )
        .expect_err("a malformed style must reject the batch");

        assert_eq!(describe(&tree), before, "the tree must be untouched");
        assert_eq!(
            tree.styles.len(),
            styles_before,
            "the failed batch must not leave styles interned"
        );
        assert!(error.contains("setStyle"), "{error}");
    }

    /// A style that fails halfway through a long batch is unfindable without
    /// its index; serde reports a byte offset, which names nothing.
    #[test]
    fn a_style_error_names_its_op_index() {
        let mut tree = RetainedTree::new();
        let error = apply(
            &mut tree,
            r#"[["createElement",1,"div"],["setStyle",1,{"color":"red"}],["setStyle",1,{"color":5}]]"#,
        )
        .expect_err("a bad style rejects the batch");
        assert!(
            error.starts_with("Batch op 2 setStyle parse error:"),
            "{error}"
        );
    }

    #[test]
    fn a_legacy_string_encoded_style_still_applies() {
        let mut tree = RetainedTree::new();
        apply(
            &mut tree,
            r#"[["createElement",1,"div"],["setStyle",1,"{\"color\":\"red\"}"]]"#,
        )
        .expect("a JSON-string style is legacy, not invalid");
        assert_eq!(
            tree.elements[&1].style.as_deref().unwrap().color.as_deref(),
            Some("red")
        );
    }

    /// `null` is not "no style". Treating it as `{}` would silently clear every
    /// declared property instead of telling JS it sent something wrong.
    #[test]
    fn a_null_style_is_an_error() {
        let mut tree = RetainedTree::new();
        let error = apply(&mut tree, r#"[["createElement",1,"div"],["setStyle",1,null]]"#)
            .expect_err("null is not a style");
        assert!(error.contains("Batch op 1 setStyle parse error:"), "{error}");
        assert!(tree.elements.is_empty(), "and the batch stays atomic");
    }

    /// Skipping an unknown opcode would let a JS/Rust version skew desync the
    /// tree quietly. It has to throw.
    #[test]
    fn an_unknown_opcode_is_an_error() {
        let mut tree = RetainedTree::new();
        let error = apply(&mut tree, r#"[["teleportElement",1]]"#).expect_err("unknown opcode");
        assert!(error.contains("unknown operation"), "{error}");
        assert!(tree.elements.is_empty());
    }

    /// Every op that takes an id must validate it. A fractional or oversized id
    /// would truncate into a *different* element, which is a silent desync.
    #[test]
    fn an_invalid_id_is_rejected_in_every_id_position() {
        let templates = [
            r#"[["createElement",ID,"div"]]"#,
            r#"[["destroyElement",ID]]"#,
            r#"[["appendChild",ID,2]]"#,
            r#"[["appendChild",1,ID]]"#,
            r#"[["removeChild",ID,2]]"#,
            r#"[["removeChild",1,ID]]"#,
            r#"[["insertBefore",ID,2,3]]"#,
            r#"[["insertBefore",1,ID,3]]"#,
            r#"[["insertBefore",1,2,ID]]"#,
            r#"[["setStyle",ID,{}]]"#,
            r#"[["setText",ID,"x"]]"#,
            r#"[["setEventListener",ID,"click",true]]"#,
            r#"[["setRoot",ID]]"#,
            r#"[["setCustomProp",ID,"k",1]]"#,
            r#"[["setCustomPropValue",ID,"k",1]]"#,
        ];
        // 1e999 overflows f64, 9007199254740992 is Number.MAX_SAFE_INTEGER + 1.
        let bad_ids = ["-1", "1.5", "9007199254740992", "1e999"];

        for template in templates {
            for bad in bad_ids {
                let json = template.replace("ID", bad);
                let mut tree = RetainedTree::new();
                let error = apply(&mut tree, &json).expect_err(&format!("{json} must be rejected"));
                assert!(error.contains("Batch op 0"), "{json}: {error}");
                assert!(tree.elements.is_empty(), "{json} mutated the tree");
                assert_eq!(tree.root_id, None, "{json} mutated the root");
            }
        }
    }

    /// The reconciler sends a bool; hand-written batches send 0 or 1. Anything
    /// else used to mean `true`, so `-1` silently registered a listener.
    #[test]
    fn has_handler_takes_a_bool_or_a_non_negative_integer() {
        for (payload, expected) in [("true", true), ("false", false), ("1", true), ("0", false)] {
            let mut tree = RetainedTree::new();
            let json = format!(
                r#"[["createElement",1,"div"],["setEventListener",1,"click",{payload}]]"#
            );
            apply(&mut tree, &json).expect("bool or non-negative integer");
            assert_eq!(
                tree.elements[&1].events.contains("click"),
                expected,
                "hasHandler {payload}"
            );
        }

        for payload in ["-1", "0.5"] {
            let mut tree = RetainedTree::new();
            let json = format!(
                r#"[["createElement",1,"div"],["setEventListener",1,"click",{payload}]]"#
            );
            apply(&mut tree, &json).expect_err(&format!("hasHandler {payload} is not a bool"));
        }
    }

    #[test]
    fn a_malformed_op_tuple_is_an_error() {
        let cases = [
            (r#"[42]"#, "a non-array op"),
            (r#"[["createElement",1]]"#, "a missing argument"),
            (r#"[[7,1,"div"]]"#, "a non-string op name"),
        ];
        for (json, what) in cases {
            let mut tree = RetainedTree::new();
            let error = apply(&mut tree, json).expect_err(what);
            assert!(error.starts_with("Failed to parse batch:"), "{what}: {error}");
            assert!(tree.elements.is_empty(), "{what} mutated the tree");
        }
    }

    /// The single-op entry points sweep too. Without that,
    /// `for (...) renderer.setStyle(1, ...)` grows the table forever.
    #[test]
    fn repeated_direct_set_style_keeps_the_table_bounded() {
        let mut tree = RetainedTree::new();
        tree.create_element(1, "div".to_string());
        for frame in 0..10_000 {
            let payload = format!(r#"{{"left":{frame}}}"#);
            tree.set_style_json(1, payload.as_bytes()).expect("valid style");
            assert!(
                tree.styles.len() <= STYLE_SWEEP_FLOOR,
                "frame {frame} left {} styles interned",
                tree.styles.len()
            );
        }
    }

    /// Interning keys on raw bytes, so re-ordered keys are two `Arc`s. They are
    /// still the same style, and a repaint per key order would be a real cost
    /// on any app that builds style objects conditionally.
    #[test]
    fn a_reordered_style_does_not_repaint() {
        let mut tree = RetainedTree::new();
        apply(
            &mut tree,
            r#"[["createElement",1,"div"],["setStyle",1,{"color":"red","left":10}]]"#,
        )
        .expect("valid batch");
        let revision = tree.elements[&1].subtree_revision;

        apply(&mut tree, r#"[["setStyle",1,{"left":10,"color":"red"}]]"#).expect("valid batch");
        assert_eq!(
            tree.elements[&1].subtree_revision, revision,
            "the same style in another key order is not a change"
        );
    }

    /// Three ways an interned style loses its last element reference.
    #[test]
    fn a_style_is_released_when_nothing_references_it() {
        let mut tree = RetainedTree::new();
        apply(&mut tree, r#"[["createElement",1,"div"]]"#).expect("valid batch");

        // Set on an id that does not exist: nothing keeps the style alive.
        apply(&mut tree, r#"[["setStyle",99,{"color":"red"}]]"#).expect("missing ids are ignored");
        tree.styles.sweep();
        assert_eq!(tree.styles.len(), 0, "a style nobody took must be released");

        apply(&mut tree, r#"[["setStyle",1,{"color":"red"}]]"#).expect("valid batch");
        tree.styles.sweep();
        assert_eq!(tree.styles.len(), 1);

        // Replaced.
        apply(&mut tree, r#"[["setStyle",1,{"color":"blue"}]]"#).expect("valid batch");
        tree.styles.sweep();
        assert_eq!(tree.styles.len(), 1, "the replaced style must be released");

        // Destroyed.
        apply(&mut tree, r#"[["destroyElement",1]]"#).expect("valid batch");
        tree.styles.sweep();
        assert_eq!(tree.styles.len(), 0);
    }
}
