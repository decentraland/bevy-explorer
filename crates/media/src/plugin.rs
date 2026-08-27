use bevy::prelude::*;

pub struct MediaPlugin;

impl Plugin for MediaPlugin {
    #[cfg_attr(not(all(target_arch = "wasm32", feature = "html")), expect(unused))]
    fn build(&self, app: &mut App) {
        #[cfg(all(target_arch = "wasm32", feature = "html"))]
        app.add_plugins(crate::html::plugin::HtmlMediaPlugin);
    }
}
