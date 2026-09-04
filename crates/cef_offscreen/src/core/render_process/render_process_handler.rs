use crate::core::prelude::{EmitBuilder, IntoString};
use crate::core::render_process::listen::ListenBuilder;
use crate::core::util::json_to_v8;
use crate::core::util::v8_accessor::V8DefaultAccessorBuilder;
use crate::core::util::v8_interceptor::V8DefaultInterceptorBuilder;
use bevy::platform::collections::HashMap;
use cef::rc::{Rc, RcImpl};
use cef::{
    Browser, Frame, ImplFrame, ImplListValue, ImplProcessMessage, ImplRenderProcessHandler,
    ImplV8Context, ImplV8Value, ProcessId, ProcessMessage, V8Context, V8Propertyattribute, V8Value,
    WrapRenderProcessHandler, sys, v8_value_create_function, v8_value_create_object,
};
use std::os::raw::c_int;
use std::sync::Mutex;

pub(crate) static LISTEN_EVENTS: Mutex<HashMap<String, V8Value>> = Mutex::new(HashMap::new());

pub const PROCESS_MESSAGE_HOST_EMIT: &str = "host-emit";
pub const PROCESS_MESSAGE_JS_EMIT: &str = "js-emit";

pub struct RenderProcessHandlerBuilder {
    object: *mut RcImpl<sys::_cef_render_process_handler_t, Self>,
}

impl RenderProcessHandlerBuilder {
    pub fn build() -> RenderProcessHandlerBuilder {
        RenderProcessHandlerBuilder {
            object: core::ptr::null_mut(),
        }
    }
}

impl WrapRenderProcessHandler for RenderProcessHandlerBuilder {
    fn wrap_rc(&mut self, object: *mut RcImpl<sys::_cef_render_process_handler_t, Self>) {
        self.object = object;
    }
}

impl Rc for RenderProcessHandlerBuilder {
    fn as_base(&self) -> &sys::cef_base_ref_counted_t {
        unsafe {
            let base = &*self.object;
            std::mem::transmute(&base.cef_object)
        }
    }
}

impl Clone for RenderProcessHandlerBuilder {
    fn clone(&self) -> Self {
        let object = unsafe {
            let rc_impl = &mut *self.object;
            rc_impl.interface.add_ref();
            rc_impl
        };
        Self { object }
    }
}

impl ImplRenderProcessHandler for RenderProcessHandlerBuilder {
    fn on_context_created(
        &self,
        _browser: Option<&mut Browser>,
        frame: Option<&mut Frame>,
        context: Option<&mut V8Context>,
    ) {
        if let Some(g) = context.and_then(|c| c.global())
            && let Some(frame) = frame
            && let Some(mut cef) = v8_value_create_object(
                Some(&mut V8DefaultAccessorBuilder::build()),
                Some(&mut V8DefaultInterceptorBuilder::build()),
            )
            && let Some(mut emit) = v8_value_create_function(
                Some(&"emit".into()),
                Some(&mut EmitBuilder::build(frame.clone())),
            )
            && let Some(mut listen) = v8_value_create_function(
                Some(&"listen".into()),
                Some(&mut ListenBuilder::build(frame.clone())),
            )
        {
            cef.set_value_bykey(
                Some(&"emit".into()),
                Some(&mut emit),
                V8Propertyattribute::default(),
            );
            cef.set_value_bykey(
                Some(&"listen".into()),
                Some(&mut listen),
                V8Propertyattribute::default(),
            );
            g.set_value_bykey(
                Some(&"cef".into()),
                Some(&mut cef),
                V8Propertyattribute::default(),
            );
        };
    }

    fn on_process_message_received(
        &self,
        _browser: Option<&mut Browser>,
        frame: Option<&mut Frame>,
        _: ProcessId,
        message: Option<&mut ProcessMessage>,
    ) -> c_int {
        if let Some(message) = message
            && let Some(frame) = frame
            && let Some(ctx) = frame.v8_context()
            && message.name().into_string() == PROCESS_MESSAGE_HOST_EMIT
        {
            handle_listen_message(message, ctx);
        };
        1
    }

    #[inline]
    fn get_raw(&self) -> *mut sys::_cef_render_process_handler_t {
        self.object.cast()
    }
}

// Backport of upstream v0.8.1's "V8 stability fixes": resolve the callback BEFORE entering the
// context, and never early-return with the context entered. The original entered the context,
// created a V8 object, then `return`ed without ctx.exit() when no listener was registered — a
// host emit arriving before the page subscribed (or during a reload) left the context entered
// and the renderer died on the next handle creation ("Cannot create a handle without a
// HandleScope", FATAL).
fn handle_listen_message(message: &ProcessMessage, mut ctx: V8Context) {
    let Some(argument_list) = message.argument_list() else {
        return;
    };
    let id = argument_list.string(0).into_string();
    let payload = argument_list.string(1).into_string();

    let callback = LISTEN_EVENTS
        .lock()
        .ok()
        .and_then(|events| events.get(&id).cloned());
    let Some(callback) = callback else {
        return; // no listener yet (page still loading) — drop the event
    };
    let Ok(value) = serde_json::from_str::<serde_json::Value>(&payload) else {
        return;
    };

    if ctx.enter() != 0 {
        let mut obj = v8_value_create_object(
            Some(&mut V8DefaultAccessorBuilder::build()),
            Some(&mut V8DefaultInterceptorBuilder::build()),
        );
        callback.execute_function_with_context(
            Some(&mut ctx),
            obj.as_mut(),
            Some(&[json_to_v8(value)]),
        );
        ctx.exit();
    }
}
