#[macro_use]
extern crate lazy_static;

pub mod args;
pub mod config;
pub mod shell;
pub mod util;

mod ci;
mod vcs;
mod module;

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }
}
