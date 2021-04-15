extern crate clap;
extern crate reqwest;
extern crate select;
extern crate ansi_term;

use reqwest::blocking::Client;
use reqwest::header::USER_AGENT;
use select::{document::Document, predicate::Name};
use std::time::Duration;
use std::sync::atomic::{AtomicBool, Ordering};
use std::path::{Path, PathBuf};
use std::fs::{File, create_dir, read_dir};
use ansi_term::Color::*;

mod cli;

//TODO: switch from reqwest to the less complex attohttpc
//TODO: To increase speed search for new links if an image has not been found or does not work.
static debug: AtomicBool = AtomicBool::new(false);
static print_numbered: AtomicBool = AtomicBool::new(true);


fn main() {
    let matches = cli::build_cli().get_matches();
    let url: &str = matches.value_of("url").expect("No url provided");
    let dir: &str;
    let dir_path: PathBuf;
    let mut number: i32 = 0;
    let mut urls: Vec<String>;
    // Contains links to images that is to be downloaded
    let mut img_links: Vec<String> = Vec::new();
    // Reqwest client to pass to get_link function
    let client = Client::builder().timeout(Duration::from_secs(60)).build().expect("Could not build Client");

    // Enables debug output if flag is present
    if matches.is_present("debug") {
        debug.store(true, Ordering::Relaxed);
    }
    if matches.is_present("not-numbered") {
        print_numbered.store(false, Ordering::Relaxed)
    }

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
    
    println!("Downloading images to {}/", Cyan.paint(dir));

    if matches.is_present("iqdb") {
        // dumps thumbnails image links on site to 'urls' to use with iqdb
        urls = get_links(&client, url).into_iter()
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
        urls = get_links(&client, url).into_iter()
                .filter(|n| (
                    n.ends_with(".jpg") || 
                    n.ends_with(".gif") || 
                    n.ends_with(".png") || 
                    n.ends_with(".jpeg") || 
                    n.ends_with(".webm") ) &&
                    !n.contains("url=") &&
                    // Filter away all thumbnail images and only keep the hi-res ones
                    !n.contains("/thumb/"))
                    .map(|n| n.replace("//", "https://"))
                .collect();
    }

    urls.dedup();

    debug_output("urls_vec", "urls filled");

    // Create directory if it does not exist
    if ! dir_path.is_dir() {
        create_dir(dir).expect("Could not create directory, may not have write permission");
    }

    for img in urls.iter() {
        // true if iqdb does not find image
        let mut iqdb_not_found: bool = false;
        // true if iqdb finds links but lynx can not find any image links 
        let mut iqdb_no_image_link_found: bool = false;
        // true if file exists in dir
        let mut iqdb_file_exists: bool = false;
        // Link to iqdb image search for current image
        let mut iqdb_link: String = String::new();
        // Name for image
        let mut name: String = String::new();
        // Path for new file
        let mut file_path: PathBuf;

        if matches.is_present("print-numbered") {
            number += 1;
        }

        // Create name for image
        name = img.split("/").filter(|&s| !s.is_empty()).last().unwrap().to_string();
        file_path = dir_path.join(&name);

        if matches.is_present("iqdb") && ( !file_path.is_file() || matches.is_present("override") ) {
            // Get name without extension or 's' for thumbnails
            name = name.replace("s", "");
            file_path = dir_path.join(name.as_str());

            // Check if a file with the same name exists (ignores file extension)
            let files = read_dir(&dir_path).expect("Could not read directory");
            let mut exists = false;
            for file in files {
                if file.unwrap().path().to_str().unwrap()
                    .contains(name.split(".").collect::<Vec<_>>()[0]) &&
                    // Ignores whether file exists or not if override flag is passed
                    !matches.is_present("override") {
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
                let mut iqdb_urls: Vec<String> = get_links(&client, &iqdb_link)
                                // Get all links before the '#' link (since all after are irrelevant)
                                .split(|n| n == &"#".to_string())
                                .collect::<Vec<_>>()[0].to_vec()
                                // Format links
                                .into_iter()
                                // Remove first element ('/' link)
                                .filter(|n| n != "/")
                                .map(|n| n.replace("//", "https://"))
                                .collect();
                
                // That site being the first link found means that the "No relevant matches" message is displayed
                if ! iqdb_urls.is_empty() && iqdb_urls[0].contains("saucenao.com/search.php") {
                    iqdb_urls = Vec::new();
                }

                debug_output("urls from iqdb", &format!("{:#?}", iqdb_urls));

                img_links = Vec::new();
                // Use all source links to find images and take all image links found
                for url in iqdb_urls.iter() {
                    debug_output("loop url", url);
                    // Create array of image links found at url given by iqdb
                    let mut new_imgs = get_links(&client, url).into_iter()
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
            }
        }
        else {
            img_links = vec!(img.to_string());
        }

        if print_numbered.load(Ordering::Relaxed) {
            number += 1;
            print!("[{}] ", Blue.paint(number.to_string()));
        }

        if ! matches.is_present("override") && ( file_path.is_file() || iqdb_file_exists ) {
            println!("{} {} in {}", 
            name.as_str(),
            Blue.paint("already exists"),
            dir);
        }
        else if iqdb_not_found {
            println!("{} on iqdb.org\n\t{}", 
            Red.paint("Image not found"),
            iqdb_link);
        }
        else if iqdb_no_image_link_found {
            println!("Image found on iqdb.org but {}\n\t{}", 
                    Yellow.paint("can not be downloaded automatically"), 
                    iqdb_link);
        }
        else {
            print!("Downloading {} to {} ", name.as_str(), dir);

            if matches.is_present("debug") {
                println!("");
            }
            
            // Iterate over found image urls until a with data is produced
            // TODO: Download from chan.sankakucomplex.com
            // TODO: Give error if no link works, check if break is called in for loop!
            for url in img_links.iter() {
                let extension = url.split(".").last().expect("No extension found");
                debug_output("extension", extension);
                file_path.set_extension(extension);

                debug_output("Trying", url.as_str());
                let client = Client::builder().timeout(Duration::from_secs(60)).build().unwrap();
                let mut resp = client.get(url)
                    .header(USER_AGENT, "4chan image downloader").send().unwrap();

                debug_output("name", &file_path.as_os_str().to_str().unwrap());
                
                let mut file: File = File::create(&file_path.as_os_str()).expect("Could not create file");
                std::io::copy(&mut resp, &mut file).expect("Could not copy to image");

                let size = std::fs::metadata(&file_path).unwrap().len();
                // Stupid solution where image must be larger than 1 kB as not to download a 404 page or something as an image
                // TODO: fix this, possible to check if image is valid?
                debug_output("size", &size.to_string());
                // Break if downloaded file contains data
                if file_path.exists() && size > 1000 {
                    break;
                }
            }

            println!("{}", Green.paint("Done"))
            // TODO: Check if file extension has changed
        }
        
    }
}

fn debug_output(title: &str, message: &str) {
    if debug.load(Ordering::Relaxed) {
        println!("[{}] {} &", Purple.paint(title), message);
    }
}

// TODO: Return Result and better error handling for connection issues, https error codes etc.
/// Returns Vector with all links found in anchor tags on given site
fn get_links(client: &Client, url: &str) -> Vec<String> {
    let mut res: Vec<String> = Vec::new();
    // TODO: proper error in case of connection error
    let resp =  match client.get(url).header(USER_AGENT, "4chan image downloader").send() {
        Ok(r) => r,
        Err(_) => return Vec::new(),
    };
    if !resp.status().is_success() {
        debug_output("status error on", url);
        return Vec::new();
    }
    assert!(resp.status().is_success(), "Connection could not be made");

    Document::from_read(resp)
        .unwrap()
        .find(Name("a"))
        .filter_map(|n| n.attr("href"))
        .for_each(|n| res.push(n.to_string()));
    return res
}