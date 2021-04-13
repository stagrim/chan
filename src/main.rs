extern crate clap;
extern crate reqwest;
extern crate select;

use select::{document::Document, predicate::Name};
// use futures::executor::block_on;

mod cli;

fn main() {
    let matches = cli::build_cli().get_matches();
    let url: &str = matches.value_of("url").expect("No url provided");
    let dir: &str;
    let mut number: i32 = 0;
    let urls: Vec<String>;

    if matches.value_of("directory").is_some() {
        dir = matches.value_of("directory").unwrap();
    }
    else {
        dir = url.split("/")
                .filter(|&s| !s.is_empty())
                .collect::<Vec<_>>()
                .last()
                .expect("No directory name could be created");
    }
    
    println!("Downloading images to {}/", dir);
    println!("Url: {}", url);

    if matches.is_present("iqdb") {
        urls = get_links(url).into_iter()
                .filter(|n| 
                    !n.contains("url=") && 
                    n.contains("/thumb/"))
                .collect();
    }
    else {
        urls = get_links(url).into_iter()
                .filter(|n| 
                    !n.contains("url=") && 
                    !n.contains("/thumb/"))
                .collect();
    }

    for url in urls {
        println!("{}", url);
    }
    
}

fn get_links(url: &str) -> Vec<String> {
    let mut res: Vec<String> = Vec::new();
    let resp = reqwest::blocking::get(url).unwrap();
    println!("{:#?}", resp);
    // assert!(resp.status().is_success(), "Connection could not be made");

    Document::from_read(resp)
        .unwrap()
        .find(Name("a"))
        .filter_map(|n| n.attr("href"))
        .filter(|n| 
            n.ends_with(".jpg") || 
            n.ends_with(".gif") || 
            n.ends_with(".png") || 
            n.ends_with(".jpeg") || 
            n.ends_with(".webm"))
        .for_each(|n| res.push(n.to_string()));
    return res
}