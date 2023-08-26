// ported from deno_fetch/lib.rs due to accessibility

use std::{borrow::Cow, rc::Rc};

use deno_core::{
    error::type_error, AsyncRefCell, AsyncResult, BufView, CancelFuture, CancelHandle, RcRef,
    WriteOutcome,
};

pub struct FetchRequestBodyResource {
    pub body: AsyncRefCell<tokio::sync::mpsc::Sender<Option<bytes::Bytes>>>,
    pub cancel: CancelHandle,
}

impl deno_core::Resource for FetchRequestBodyResource {
    fn name(&self) -> Cow<str> {
        "fetchRequestBody".into()
    }

    fn write(self: Rc<Self>, buf: BufView) -> AsyncResult<WriteOutcome> {
        Box::pin(async move {
            let bytes: bytes::Bytes = buf.into();
            let nwritten = bytes.len();
            let body = RcRef::map(&self, |r| &r.body).borrow_mut().await;
            let cancel = RcRef::map(self, |r| &r.cancel);
            body.send(Some(bytes))
                .or_cancel(cancel)
                .await?
                .map_err(|_| type_error("request body receiver not connected (request closed)"))?;
            Ok(WriteOutcome::Full { nwritten })
        })
    }

    fn shutdown(self: Rc<Self>) -> AsyncResult<()> {
        Box::pin(async move {
            let body = RcRef::map(&self, |r| &r.body).borrow_mut().await;
            let cancel = RcRef::map(self, |r| &r.cancel);
            // There is a case where hyper knows the size of the response body up
            // front (through content-length header on the resp), where it will drop
            // the body once that content length has been reached, regardless of if
            // the stream is complete or not. This is expected behaviour, but it means
            // that if you stream a body with an up front known size (eg a Blob),
            // explicit shutdown can never succeed because the body (and by extension
            // the receiver) will have dropped by the time we try to shutdown. As such
            // we ignore if the receiver is closed, because we know that the request
            // is complete in good health in that case.
            body.send(None).or_cancel(cancel).await?.ok();
            Ok(())
        })
    }

    fn close(self: Rc<Self>) {
        self.cancel.cancel()
    }
}

// cheat and read the whole thing
pub struct FetchResponseBodyResource {
    pub data: AsyncRefCell<bytes::Bytes>,
    pub cancel: CancelHandle,
    pub size: Option<u64>,
}

impl deno_core::Resource for FetchResponseBodyResource {
    fn name(&self) -> Cow<str> {
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
