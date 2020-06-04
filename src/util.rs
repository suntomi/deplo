// defer
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

// func
#[macro_export]
macro_rules! macro_func {
    () => {{
        fn f() {}
        fn type_name_of<T>(_: T) -> &'static str {
            std::any::type_name::<T>()
        }
        let name = type_name_of(f);
        &name[..name.len() - 3]
    }}
}
pub use macro_func as func;

// error
use std::fmt;
use std::error::Error;

#[derive(Debug)]
pub struct EscalateError {
    pub at: (&'static str, &'static str, u32),
    pub source: Box<dyn Error + 'static>
}
impl fmt::Display for EscalateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.source() {
            Some(err) => write!(f, "{} {}:{}\n{}", self.at.0, self.at.1, self.at.2, err),
            None => write!(f, "{} {}:{}\n", self.at.0, self.at.1, self.at.2),
        }
    }
}
impl Error for EscalateError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        Some(self.source.as_ref())
    }
}

#[macro_export]
macro_rules! macro_escalate  {
    ( $err:expr ) => {
        Err(Box::new(crate::util::EscalateError {
            at: (crate::util::func!(), file!(), line!()),
            source: $err
        }))
    }
}
pub use macro_escalate as escalate;

// envsubst
use regex::{Regex,Captures};
use std::collections::HashMap;

pub fn envsubst(src: &str) -> String {
    let envs: HashMap<String, String> = std::env::vars().collect();
    let re = Regex::new(r"\$\{([^\}]+)\}").unwrap();
    let content = re.replace_all(src, |caps: &Captures| {
        match envs.get(&caps[1]) {
            Some(s) => s.replace("\n", r"\n").replace(r"\", r"\\").replace(r#"""#, r#"\""#),
            None => return caps[1].to_string()
        }
    });
    return content.to_string()
}