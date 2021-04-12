extern crate clap;

use std::fs::{File, create_dir, remove_dir_all};
use clap::Shell;

include!("src/cli.rs");

pub const NAME: &str = "chan";

fn main() {
    let mut app = build_cli();

    remove_dir_all("completion").unwrap();
    create_dir("completion").unwrap();
    app.gen_completions_to(NAME, Shell::Bash, &mut File::create(format!("completion/{}", NAME)).unwrap());
    app.gen_completions_to(NAME, Shell::Zsh, &mut File::create(format!("completion/_{}", NAME)).unwrap());
}