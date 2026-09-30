use crate::core::prelude::{HOST_CEF, SCHEME_CEF};
use bevy::prelude::*;

pub(crate) struct WebviewCoreComponentsPlugin;

impl Plugin for WebviewCoreComponentsPlugin {
    fn build(&self, app: &mut App) {
        app.register_type::<WebviewSize>()
            .register_type::<CefWebviewUri>()
            .register_type::<HostWindow>();
    }
}

/// A component that specifies the URI of the webview.
///
/// When opening a remote web page, specify the URI with the http(s) schema.
///
/// When opening a local file, use the custom schema `cef://localhost/` to specify the file path.
/// Alternatively, you can also use [`CefWebviewUri::local`].
#[derive(Component, Debug, Clone, PartialEq, Eq, Hash, Reflect)]
#[reflect(Component, Debug)]
#[require(WebviewSize)]
pub struct CefWebviewUri(pub String);

impl CefWebviewUri {
    /// Creates a new `CefWebviewUri` with the given URI.
    ///
    /// If you want to specify a local file path, use [`CefWebviewUri::local`] instead.
    pub fn new(uri: impl Into<String>) -> Self {
        Self(uri.into())
    }

    /// Creates a new `CefWebviewUri` with the given file path.
    ///
    /// It interprets the given path as a file path in the format `cef://localhost/<file_path>`.
    pub fn local(uri: impl Into<String>) -> Self {
        Self(format!("{SCHEME_CEF}://{HOST_CEF}/{}", uri.into()))
    }
}

/// Specifies the view size of the webview.
///
/// This does not affect the actual object size.
#[derive(Reflect, Component, Debug, Copy, Clone, PartialEq)]
#[reflect(Component, Debug, Default)]
pub struct WebviewSize(pub Vec2);

impl Default for WebviewSize {
    fn default() -> Self {
        Self(Vec2::splat(800.0))
    }
}

/// An optional component to specify the parent window of the webview.
/// The window handle of [Window] specified by this component is passed to `parent_view` of [`WindowInfo`](cef::WindowInfo).
///
/// If this component is not inserted, the handle of [PrimaryWindow](bevy::window::PrimaryWindow) is passed instead.
#[derive(Reflect, Component, Debug, Copy, Clone, PartialEq)]
#[reflect(Component, Debug)]
pub struct HostWindow(pub Entity);
