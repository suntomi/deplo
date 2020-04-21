mod config;
mod args;
mod cli;

fn main() {
    let args = args::Args::create().unwrap();
    let c = config::Config::create(&args).unwrap();
    std::process::exit(
        cli::run(&args, &c).unwrap()
    );
}
