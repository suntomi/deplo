#[macro_use]
extern crate lazy_static;

pub mod args;
pub mod config;
pub mod secret;
pub mod shell;
pub mod util;
pub mod step;
pub mod vcs;
pub mod ci;
pub mod workflow;

mod module;

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }
}
