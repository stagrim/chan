extern crate clap;
extern crate reqwest;
extern crate select;

use reqwest::blocking::Client;
use select::{document::Document, predicate::Name};
use std::time::Duration;
use reqwest::header::USER_AGENT;
use std::path::{Path, PathBuf};
use std::fs::{File, create_dir, remove_dir_all, read_dir};

mod cli;

fn main() {
    let matches = cli::build_cli().get_matches();
    let url: &str = matches.value_of("url").expect("No url provided");
    let dir: &str;
    let dir_path: PathBuf;
    let mut number: i32 = 0;
    let mut urls: Vec<String>;
    // Contains links to images that is to be downloaded
    let mut img_links: Vec<String> = Vec::new();

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
    dir_path = Path::new(".").join(dir);
    
    println!("Downloading images to {}/", dir);
    println!("Url: {}", url);

    if matches.is_present("iqdb") {
        // dumps thumbnails image links on site to 'urls' to use with iqdb
        urls = get_links(url).into_iter()
                // Grab only thumbnail images
                .filter(|n| n.contains("/thumb/"))
                // Split at http to separate the two links and get the second one
                .map(|n|
                    format!("http{}", n.split("http")
                    .filter(|&s| !s.is_empty())
                    .filter(|&n| (
                        n.ends_with(".jpg") || 
                        n.ends_with(".gif") || 
                        n.ends_with(".png") || 
                        n.ends_with(".jpeg") || 
                        n.ends_with(".webm") ) &&
                        !n.contains("url="))
                    .collect::<Vec<_>>()
                    .last().unwrap().to_string()))
                .collect();
    }
    else {
        urls = get_links(url).into_iter()
                .filter(|n| (
                    n.ends_with(".jpg") || 
                    n.ends_with(".gif") || 
                    n.ends_with(".png") || 
                    n.ends_with(".jpeg") || 
                    n.ends_with(".webm") ) &&
                    !n.contains("url=") &&
                    // Filter away all thumbnail images and only keep the hi-res ones
                    !n.contains("/thumb/"))
                .collect();
    }

    urls.dedup();

    debug_output("urls_vec", "urls filled");

    // Create directory if it does not exist
    if ! dir_path.is_dir() {
        create_dir(dir).expect("Could not create directory, may not have write permission");
    }

    for img in urls {
        // true if iqdb does not find image
        let mut iqdb_not_found: bool = false;
        // true if iqdb finds links but lynx can not find any image links 
        let mut iqdb_no_image_link_found: bool = false;
        // true if file exists in dir
        let mut iqdb_file_exists: bool = false;
        // Link to iqdb image search for current image
        let mut iqdb_link: String = String::new();
        // Name for image
        let mut name: &str;
        // Path for new file
        let file_path: PathBuf;

        if matches.is_present("print-numbered") {
            number += 1;
        }

        // Create name for image
        name = img.split("/").filter(|&s| !s.is_empty()).last().unwrap();
        file_path = dir_path.join(name);

        if matches.is_present("iqdb") && ! file_path.is_file() {
            // Get name without extension or 's' for thumbnails
            let name = &name.replace("s", "");

            // Check if a file with the same name exists (ignores file extension)
            let files = read_dir(&dir_path).expect("Could not read directory");
            let mut exists = false;
            for file in files {
                if file.unwrap().path().to_str().unwrap()
                    .contains(name.split(".").collect::<Vec<_>>()[0]) {
                        exists = true
                }
            }
            if exists {
                iqdb_file_exists = true;
            }
            else {
                // Getting extension here should not be necessary 

                debug_output("img", &img.clone());

                // Create link to iqdb image search for current image (used if no image is found)
                // TODO: grab iqdb link directly from site?
                iqdb_link = format!("https://iqdb.org/?url={}", img).clone();
                debug_output("iqdb_link", &iqdb_link);
                
                // Lists all links on site and removes non useful links
                let iqdb_urls: Vec<String> = get_links(&iqdb_link)
                                // Get all links before the '#' link (since all after are irrelevant)
                                .split(|n| n == &"#".to_string())
                                .collect::<Vec<_>>()[0].to_vec()
                                // Remove first element ('/' link) Crashed if result is empty
                                // TODO: use filter instead
                                .split_first()
                                    // Some sort of error handling
                                    .or(Some((&"".to_string(), &vec!("".to_string()))))
                                    .unwrap().1.to_vec()
                                // Format links
                                .into_iter()
                                .map(|n| n.replace("//", "https://"))
                                .collect();

                debug_output("urls from iqdb", &format!("{:#?}", iqdb_urls));

                img_links = Vec::new();
                // Use all source links to find images and take all image links found
                for url in iqdb_urls.iter() {
                    debug_output("loop url", url);
                    // Create array of image links found at url given by iqdb
                    let mut new_imgs = get_links(url).into_iter()
                                                .filter(|n| (
                                                    n.ends_with(".jpg") || 
                                                    n.ends_with(".gif") || 
                                                    n.ends_with(".png") || 
                                                    n.ends_with(".jpeg") || 
                                                    n.ends_with(".webm") ) &&
                                                    !n.contains("url="))
                                                .collect::<Vec<_>>();
                    debug_output("new imgs", &format!("{:#?}", new_imgs));

                    img_links.append(&mut new_imgs);
                }

                debug_output("img links", &format!("{:#?}", img_links));

                // If no image is found
                if iqdb_urls.is_empty() {
                    iqdb_not_found = true;
                }
                if img_links.is_empty() {
                    iqdb_no_image_link_found = true;
                }
                // Newly found image may have another file extension
                // Updates name with new extension
                debug_output("name", name);
            }
        }

        if matches.is_present("print-numbered") {
            print!("[{}] ", number);
        }

        if ! matches.is_present("override") && ( file_path.is_file() || iqdb_file_exists ) {
            print!("{} already exists in {}", name, dir);
        }
        else if iqdb_not_found {
            print!("Image not found on iqdb.org\n\t{}", iqdb_link);
        }
        else if iqdb_no_image_link_found {
            print!("Image found on iqdb.org but can not be downloaded automatically\n\t{}", iqdb_link);
        }
        else {
            print!("Downloading {} to {}", name, dir);

            if matches.is_present("debug") {
                println!("");
            }
            // Iterate over found image urls until a with data is produced
            // TODO: Download from chan.sankakucomplex.com
            // TODO: Give error if no link works, check if break is called in for loop!

            for url in img_links.iter() {
                debug_output("Trying", url.as_str());
                let client = Client::builder().timeout(Duration::from_secs(60)).build() .unwrap();
                let mut resp = client.get(url)
                    .header(USER_AGENT, "4chan image downloader").send().unwrap();
                debug_output("path", file_path.as_path().to_str().unwrap());
                let mut file: File = File::create("test.jpg").expect("Could not create file");
                std::io::copy(&mut resp, &mut file).expect("failed to copy content");
            }
            // TODO: Check if file extension has changed
        }
        
    }
}

fn debug_output(title: &str, message: &str) {
    println!("[{}] {} &", title, message);
}

fn get_links(url: &str) -> Vec<String> {
    let mut res: Vec<String> = Vec::new();
    // TODO: proper error in case of connection error
    let client = Client::builder().timeout(Duration::from_secs(60)).build().unwrap();
    let resp = client.get(url).header(USER_AGENT, "4chan image downloader").send().unwrap();
    assert!(resp.status().is_success(), "Connection could not be made");

    Document::from_read(resp)
        .unwrap()
        .find(Name("a"))
        .filter_map(|n| n.attr("href"))
        .for_each(|n| res.push(n.to_string()));
    return res
}