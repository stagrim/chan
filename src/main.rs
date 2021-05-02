extern crate clap;
extern crate attohttpc;
extern crate select;
extern crate ansi_term;
extern crate filetime;

use attohttpc::Response;
use select::{document::Document, predicate::{Class, Name}};
use std::{io::Write, sync::atomic::{AtomicBool, Ordering}};
use std::path::{Path, PathBuf};
use std::fs::{File, create_dir, read_dir, read_to_string};
use ansi_term::Color::*;
use filetime::set_file_mtime;

mod cli;

//TODO: To increase speed search for new links if an image has not been found or does not work.
//TODO: Download file as tmp file that autodeletes until fully downloaded
//TODO: Add progress bar
static DEBUG: AtomicBool = AtomicBool::new(false);
static PRINT_NUMBERED: AtomicBool = AtomicBool::new(true);

const USER_AGENT: &str = "user-agent";
const USER_AGENT_VALUE: &str = "Mozilla/5.0 (X11; Linux x86_64; rv:87.0) Gecko/20100101 Firefox/87.0";


fn main() {
    let matches = cli::build_cli().get_matches();
    let mut update_modify_date: bool = false;
    let url: String = matches.value_of("url").expect("No url provided").to_string();

    // Enables debug output if flag is present
    if matches.is_present("debug") {
        DEBUG.store(true, Ordering::Relaxed);
    }
    // Disables numbered output when flag is passed
    if matches.is_present("not-numbered") {
        PRINT_NUMBERED.store(false, Ordering::Relaxed)
    }
    if matches.is_present("update-modify-date") {
        update_modify_date = true;
    }

    let mut threads = match file_to_vec() {
        Ok(f) => f,
        Err(_) => Vec::new(),
    };

    match matches.subcommand() {
        ("update", _) => {
            // TODO: Implement update subcommand
            println!("{:#?}", threads);
        },
        _ => {
            // Add link to threads file for 'update' subcommand if not present
            threads.push(url.to_string());
            threads.dedup();
            vec_to_file(threads);

            chan(&url, 
                update_modify_date, 
                matches.value_of("directory"), 
                matches.is_present("iqdb"),
                matches.is_present("override")
            );
        }
    }
}

// The procedure of grabbing information to downloading the images from the thread
fn chan<S: AsRef<str>>(
        url: S, 
        update_modify_date: bool,
        param_dir: Option<&str>,
        iqdb: bool,
        override_enabled: bool
) {
    let url: String = url.as_ref().to_string();
    let dir: String;
    let dir_path: PathBuf;
    let mut number: i32 = 0;
    let mut urls: Vec<String>;
    // Contains links to images that is to be downloaded
    let img_links: Vec<String> = Vec::new();


    if param_dir.is_some() {
        dir = param_dir.unwrap().to_string();
        // TODO: Add other non permitted characters
        if dir.contains("/") {
            println!("{} directory cannot contain '/' character", Red.paint("Error:"));
            std::process::exit(1);
        }
    }
    else {
        dir = get_name(&url);
    }
    dir_path = Path::new(".").join(&dir);
    
    println!("Downloading images to {}/", Cyan.paint(&dir));

    if iqdb {
        // dumps thumbnails image links on site to 'urls' to use with iqdb
        urls = get_links(&url).into_iter()
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
        urls = get_links(&url).into_iter()
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
        create_dir(&dir).expect("Could not create directory, may not have write permission");
    }

    for img in urls.iter() {
        // Path for new file
        let file_path: PathBuf;

        if PRINT_NUMBERED.load(Ordering::Relaxed) {
            number += 1;
            print!("[{}] ", Blue.paint(number.to_string()));
        }
        
        // Downloads file
        file_path = download(
            &dir_path,
            &dir, &img, 
            img_links.clone(), 
            override_enabled,
            iqdb
        );

        if update_modify_date {
            debug_output("file path", file_path.to_str().unwrap());
            
            set_file_mtime(file_path, filetime::FileTime::now()).expect("Could not update modified date");
        }
        
    }
}

fn debug_output(title: &str, message: &str) {
    if DEBUG.load(Ordering::Relaxed) {
        println!("[{}] {} &", Purple.paint(title), message);
    }
}

/// Downloads file and returns file path of the downloaded file
fn download<P: AsRef<Path>, S: AsRef<str>>(
            dir_path: P,
            dir: S,
            img: S,
            mut img_links: Vec<String>,
            override_enabled: bool,
            iqdb: bool
    ) -> PathBuf {

    // true if iqdb does not find image
    let mut iqdb_not_found: bool = false;
    // true if file exists in dir
    let mut iqdb_file_exists: bool = false;
    // true if iqdb finds links but lynx can not find any image links 
    let mut iqdb_no_image_link_found: bool = false;
    // Link to iqdb image search for current image
    let mut iqdb_link: String = String::new();
    // Name for image
    let mut name: String;
    // Path for new file
    let mut file_path: PathBuf;
    // Name without an extension
    let file_name : String;

    // Create name for image
    name = img.as_ref().split("/").filter(|&s| !s.is_empty()).last().unwrap().to_string();
    file_name = name.split(".").collect::<Vec<_>>()[0].to_string();
    file_path = dir_path.as_ref().join(&name);

    if iqdb && ( !file_path.is_file() || override_enabled ) {
        // Get name without extension or 's' for thumbnails
        name = name.replace("s", "");
        file_path = dir_path.as_ref().join(name.as_str());

        // Check if a file with the same name exists (ignores file extension)
        let files = read_dir(&dir_path).expect("Could not read directory");
        let mut exists = false;
        for file in files {
            if file.unwrap().path().to_str().unwrap().contains(&file_name) && ! override_enabled {
                    exists = true;
                    break;
            }
        }
        
        if exists {
            iqdb_file_exists = true;
        }
        else {
            // Getting extension here should not be necessary 

            debug_output("img", &img.as_ref().clone());

            // Create link to iqdb image search for current image (used if no image is found)
            // TODO: grab iqdb link directly from site?
            iqdb_link = format!("https://iqdb.org/?url={}", img.as_ref());
            debug_output("iqdb_link", &iqdb_link);
            
            // Lists all links on site and removes non useful links
            let mut iqdb_urls: Vec<String> = get_links( &iqdb_link)
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
        }
    }
    else {
        img_links = vec!(img.as_ref().to_string());
    }

    if ! override_enabled && ( file_path.is_file() || iqdb_file_exists ) {
        for entry in read_dir(&dir_path).unwrap() {
            let entry_file_name = entry.unwrap().file_name();
            if entry_file_name.to_str().unwrap().contains(&file_path.to_str().unwrap()) {
                //TODO: Fix when iqdb comes online
                println!("{:?}", entry_file_name);
            }
        }
    
        println!("{} {} in {}", 
        name.as_str(),
        Blue.paint("already exists"), dir.as_ref());
    }
    else if iqdb_not_found {
        println!("{} on iqdb.org\n\t{}", 
        Red.paint("Image not found"),
        &iqdb_link);
    }
    else if iqdb_no_image_link_found {
        println!("Image found on iqdb.org but {}\n\t{}", 
                Yellow.paint("can not be downloaded automatically"), 
                &iqdb_link);
    }
    else {
        print!("Downloading {} to {} ", name.as_str(), dir.as_ref());

        if DEBUG.load(Ordering::Relaxed) {
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

            // TODO: Better error handling
            let mut resp: Response = attohttpc::get(url)
                .header(USER_AGENT, USER_AGENT_VALUE).send().unwrap();

            debug_output("name", &file_path.as_os_str().to_str().unwrap());
            
            let mut file: File = File::create(&file_path.as_os_str()).expect("Could not create file");
            std::io::copy(&mut resp, &mut file).expect("Could not download image to file");

            let size = std::fs::metadata(&file_path).unwrap().len();
            // Stupid solution where image must be larger than 1 kB as not to download a 404 page or something as an image
            // TODO: fix this, possible to check if image is valid?
            debug_output("size", &size.to_string());
            // Break if downloaded file contains data
            if file_path.exists() && size > 1000 {
                break;
            }
        }

        println!("{}", Green.paint("Done"));
    }
    return file_path;
}

/// Returns all lines in a file as a Vector
fn file_to_vec() -> Result<Vec<String>, String> {
    let res: Vec<String>;
    let contents;
    // TODO: Error handling when no file found
    contents = read_to_string("threads.txt")
        .expect("threads.txt not found");
    res = contents.split_whitespace().map(|s| s.trim().to_string()).collect::<Vec<String>>();
    return Ok(res);
}

/// Returns HTML Document of given site
fn get_html(url: &str) -> Result<Document, String> {
    // TODO: proper error in case of connection error
    let resp: Response =  match attohttpc::get(url).header(USER_AGENT, USER_AGENT_VALUE).send() {
        Ok(r) => r,
        Err(_) => {
                    debug_output("status error on", url);
                    return Err(format!("Device may be offline"));
                },
    };

    if !resp.is_success() {
        debug_output("status error on", url);
        return Err(format!("Site returned {} error status code", resp.status()))
    }

    let document = Document::from_read(resp).unwrap();

    return Ok(document);
}

// TODO: Return Result and better error handling for connection issues, https error codes etc.
/// Returns Vector with all links found in anchor tags on given site
fn get_links(url: &str) -> Vec<String> {
    let mut res: Vec<String> = Vec::new();

    get_html(url)
        .unwrap()
        .find(Name("a"))
        .filter_map(|n| n.attr("href"))
        .for_each(|n| res.push(n.to_string()));
    return res
}

/// Returns a folder name with the "{thread-id} - {thread subject}" pattern
fn get_name<S: AsRef<str>>(url: S) -> String {
    let doc = get_html(&url.as_ref()).expect("Could not fetch site");
        let thread_number: String = url.as_ref().split("/").filter(|&s| !s.is_empty()).collect::<Vec<_>>().last().unwrap().to_string();
        let mut subject_node = doc.find(Class("subject")).collect::<Vec<_>>();

        // TODO: More elegant solution instead of if, if, if ...
        if subject_node.first().unwrap().text().is_empty() {
            subject_node = doc.find(Class("name")).collect::<Vec<_>>();
        }
        if subject_node.first().unwrap().text().is_empty() {
            // Class on archived.moe
            subject_node = doc.find(Class("post_title")).collect::<Vec<_>>();
        }

        let subject: String;
        if subject_node.first().unwrap().text().is_empty() {
            subject = "title".to_string();
        }
        else {
            subject = subject_node.first().unwrap().text().replace("/", " ");
        }

        return format!("{} - {}", thread_number, subject);
}

/// Removes existing file and writes links from Vec to it
fn vec_to_file(vec: Vec<String>) {
    // std::fs::remove_file("threads.txt");
    let mut file = File::create("threads.txt").expect("Could not create file");

	for l in vec.iter() {
		file.write(format!("{}\n", l).as_bytes()).expect("Could not write to file");
	}
}