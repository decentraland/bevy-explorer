pub trait ReplaceIfSome
where
    Self: Sized,
{
    fn replace_if_some(&mut self, other: Option<Self>);
}

impl<T> ReplaceIfSome for T {
    fn replace_if_some(&mut self, other: Option<Self>) {
        if let Some(other) = other {
            let _ = core::mem::replace(self, other);
        }
    }
}
