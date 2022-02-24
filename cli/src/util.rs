// escalate
#[macro_export]
macro_rules! macro_escalate {
    ( $err:expr ) => {
        Err(Box::new(core::util::EscalateError {
            at: (core::util::func!(), file!(), line!()),
            source: $err
        }))
    }
}
pub use macro_escalate as escalate;

// defer
#[macro_export]
macro_rules! macro_defer_expr { ($e: expr) => { $e } } // tt hack
#[macro_export]
macro_rules! macro_defer {
    ($($data: tt)*) => (
        let _scope_call = core::util::ScopedCall {
            c: Some(|| -> () { $crate::macro_defer_expr!({ $($data)* }) })
        };
    )
}
pub use macro_defer as defer;
