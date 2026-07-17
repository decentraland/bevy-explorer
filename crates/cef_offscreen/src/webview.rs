use crate::components::{CefWebviewUri, HostWindow, WebviewSize};
use crate::core::prelude::*;
use crate::cursor_icon::SystemCursorIconSender;
use crate::ipc::IpcEventRawSender;
use bevy::ecs::component::HookContext;
use bevy::ecs::world::DeferredWorld;
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use bevy::winit::WinitWindows;
#[allow(deprecated)]
use raw_window_handle::HasRawWindowHandle;
use serde::{Deserialize, Serialize};

pub mod prelude {
    pub use crate::webview::{RequestCloseDevtool, RequestShowDevTool, WebviewPlugin};
}

/// A Trigger event to request showing the developer tools in a webview.
///
/// When you want to close the developer tools, use [`RequestCloseDevtool`].
///
/// ```rust
/// use bevy::prelude::*;
/// use cef_offscreen::prelude::*;
///
/// #[derive(Component)]
/// struct DebugWebview;
///
/// fn show_devtool_system(mut commands: Commands, webviews: Query<Entity, With<DebugWebview>>) {
///     commands.entity(webviews.single().unwrap()).trigger(RequestShowDevTool);
/// }
/// ```
#[derive(Reflect, Debug, Default, Copy, Clone, Serialize, Deserialize, Event)]
#[reflect(Default, Serialize, Deserialize)]
pub struct RequestShowDevTool;

/// A Trigger event to request closing the developer tools in a webview.
///
/// When showing the devtool, use [`RequestShowDevTool`] instead.
///
/// ```rust
/// use bevy::prelude::*;
/// use cef_offscreen::prelude::*;
///
/// #[derive(Component)]
/// struct DebugWebview;
///
/// fn close_devtool_system(mut commands: Commands, webviews: Query<Entity, With<DebugWebview>>) {
///    commands.entity(webviews.single().unwrap()).trigger(RequestCloseDevtool);
/// }
/// ```
#[derive(Reflect, Debug, Default, Copy, Clone, Serialize, Deserialize, Event)]
#[reflect(Default, Serialize, Deserialize)]
pub struct RequestCloseDevtool;

pub struct WebviewPlugin;

impl Plugin for WebviewPlugin {
    fn build(&self, app: &mut App) {
        app.register_type::<RequestShowDevTool>()
            .init_non_send_resource::<Browsers>()
            // the OSR texture pump lives here (not in the worldspace material plugins) so that
            // fullscreen/offscreen consumers get RenderTexture events without worldspace webviews
            .add_event::<RenderTexture>()
            .add_systems(Update, send_render_textures)
            .add_systems(
                Update,
                (
                    resize.run_if(any_resized),
                    create_webview.run_if(added_webview),
                ),
            )
            .add_observer(apply_request_show_devtool)
            .add_observer(apply_request_close_devtool);

        // macOS: the main thread is the CEF UI thread; pump explicit begin-frames in
        // step with the app frame loop. Windows/Linux paint on CEF's own schedule.
        #[cfg(target_os = "macos")]
        app.add_systems(Main, send_external_begin_frame);

        // Windows/Linux: browser objects live on CEF's UI thread (multi-threaded message
        // loop). CEF is already initialized — MessageLoopPlugin::default() ran when the
        // plugin tuple was constructed — so set up that thread's browser state now, and
        // flush the frame's queued commands to it in Last (after Update systems,
        // observers and hooks have enqueued).
        #[cfg(not(target_os = "macos"))]
        {
            app.world().non_send_resource::<Browsers>().post_init_task();
            app.add_systems(Last, post_drain_commands);
        }

        app.world_mut()
            .register_component_hooks::<CefWebviewUri>()
            .on_despawn(|mut world: DeferredWorld, ctx: HookContext| {
                world.non_send_resource_mut::<Browsers>().close(&ctx.entity);
            });
    }
}

fn any_resized(webviews: Query<Entity, Changed<WebviewSize>>) -> bool {
    !webviews.is_empty()
}

fn added_webview(webviews: Query<Entity, Added<CefWebviewUri>>) -> bool {
    !webviews.is_empty()
}

#[cfg(target_os = "macos")]
fn send_external_begin_frame(mut hosts: NonSendMut<Browsers>) {
    hosts.send_external_begin_frame();
}

#[cfg(not(target_os = "macos"))]
fn post_drain_commands(browsers: NonSend<Browsers>) {
    if browsers.commands_pending() {
        browsers.post_drain_task();
    }
}

fn send_render_textures(mut ew: EventWriter<RenderTexture>, browsers: NonSend<Browsers>) {
    while let Ok(texture) = browsers.try_receive_texture() {
        ew.write(texture);
    }
}

#[allow(clippy::too_many_arguments)]
fn create_webview(
    mut browsers: NonSendMut<Browsers>,
    requester: Res<Requester>,
    ipc_event_sender: Res<IpcEventRawSender>,
    cursor_icon_sender: Res<SystemCursorIconSender>,
    winit_windows: NonSend<WinitWindows>,
    webviews: Query<
        (Entity, &CefWebviewUri, &WebviewSize, Option<&HostWindow>),
        Added<CefWebviewUri>,
    >,
    primary_window: Query<Entity, With<PrimaryWindow>>,
) {
    for (entity, uri, size, parent) in webviews.iter() {
        let host_window = parent
            .and_then(|w| winit_windows.get_window(w.0))
            .or_else(|| winit_windows.get_window(primary_window.single().ok()?))
            .and_then(|w| {
                #[allow(deprecated)]
                w.raw_window_handle().ok()
            });
        browsers.create_browser(
            entity,
            &uri.0,
            size.0,
            requester.clone(),
            ipc_event_sender.0.clone(),
            cursor_icon_sender.clone(),
            host_window,
        );
    }
}

fn resize(
    browsers: NonSend<Browsers>,
    webviews: Query<(Entity, &WebviewSize), Changed<WebviewSize>>,
) {
    for (webview, size) in webviews.iter() {
        browsers.resize(&webview, size.0);
    }
}

fn apply_request_show_devtool(trigger: Trigger<RequestShowDevTool>, browsers: NonSend<Browsers>) {
    browsers.show_devtool(&trigger.target());
}

fn apply_request_close_devtool(trigger: Trigger<RequestCloseDevtool>, browsers: NonSend<Browsers>) {
    browsers.close_devtools(&trigger.target());
}
