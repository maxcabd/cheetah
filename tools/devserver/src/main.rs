//! cheetah-devserver: serve an app compiled into the runtime, by name.
//!
//!   cargo run -p cheetah-devserver -- --app chat
//!
//! For instant reloads of an app's own code, run its crate instead, e.g.
//! `dev-hot.ps1 (dx serve --hot-patch -p nightshade)` (apps/nightshade).

use cheetah_devserver::{run, AppSource, Options};

fn main() {
    let options = Options::from_args(
        "usage: cheetah-devserver [--app chat] [--port N] [--lan]
         Nightshade runs from its own crate: dev-hot.ps1 (dx serve --hot-patch -p nightshade)",
    );
    let mut app = "chat".to_string();
    let mut rest = options.rest.iter();
    while let Some(arg) = rest.next() {
        match arg.as_str() {
            "--app" => match rest.next() {
                Some(name) => app = name.clone(),
                None => {
                    eprintln!("--app needs a name");
                    std::process::exit(2);
                }
            },
            other => {
                eprintln!("unknown argument {other:?}; see --help");
                std::process::exit(2);
            }
        }
    }
    run(&app.clone(), options, AppSource::Named(app));
}
