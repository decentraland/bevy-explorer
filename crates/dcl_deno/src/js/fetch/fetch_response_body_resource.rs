// ported from deno_fetch/lib.rs due to accessibility

use std::{borrow::Cow, rc::Rc};

use deno_core::{AsyncRefCell, AsyncResult, BufView, CancelHandle, RcRef};

// cheat and read the whole thing
pub struct FetchResponseBodyResource {
    pub data: AsyncRefCell<bytes::Bytes>,
    pub cancel: CancelHandle,
    pub size: Option<u64>,
}

impl deno_core::Resource for FetchResponseBodyResource {
    fn name(&self) -> Cow<'_, str> {
        "fetchResponseBody".into()
    }

    fn read(self: Rc<Self>, limit: usize) -> AsyncResult<BufView> {
        Box::pin(async move {
            let mut chunk = RcRef::map(&self, |r| &r.data).borrow_mut().await;
            let len = chunk.len();
            if len == 0 {
                return Ok(BufView::empty());
            }

            Ok(chunk.split_to(limit.min(len)).into())
        })
    }

    fn size_hint(&self) -> (u64, Option<u64>) {
        (self.size.unwrap_or(0), self.size)
    }

    fn close(self: Rc<Self>) {
        self.cancel.cancel()
    }
}
