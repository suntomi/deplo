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
