extern crate clap;

mod cli;

fn main() {
    let matches = cli::build_cli().get_matches();
    
    println!("Iqdb: {}", matches.is_present("Iqdb"));
    println!("Directory: {}", matches.is_present("Directory"));
    if matches.is_present("Directory") {
        println!("\tValue: {}", matches.value_of("Directory").unwrap());
    }
    println!("Override: {}", matches.is_present("Override"));
    println!("Print-numbered: {}", matches.is_present("Print-numbered"));
    println!("Quiet: {}", matches.is_present("Quiet"));
    println!("Debug: {}", matches.is_present("Debug"));
    println!("Url: {}", matches.value_of("URL").unwrap());
}