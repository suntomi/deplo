pub struct ScopedCall<F: FnOnce()> {
    pub c: Option<F>
}
impl<F: FnOnce()> Drop for ScopedCall<F> {
    fn drop(&mut self) {
        self.c.take().unwrap()()
    }
}

#[macro_export]
macro_rules! macro_defer_expr { ($e: expr) => { $e } } // tt hack
#[macro_export]
macro_rules! macro_defer {
    ($($data: tt)*) => (
        let _scope_call = crate::util::ScopedCall {
            c: Some(|| -> () { $crate::macro_defer_expr!({ $($data)* }) })
        };
    )
}

pub use macro_defer as defer;