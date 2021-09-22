extern crate clap;
extern crate reqwest;
extern crate select;
extern crate ansi_term;
extern crate filetime;

use reqwest::blocking::{Response, Client};
use select::{document::Document, predicate::{Class, Name}};
use tempfile::NamedTempFile;
use core::time;
use std::{process, sync::atomic::{AtomicBool, Ordering}, thread, time::SystemTime};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::fs::{File, create_dir, read_dir, read_to_string, copy};
use ansi_term::Color::*;
use filetime::{FileTime, set_file_mtime};

mod cli;

// Mostly ideas for new features
//TODO: Add flag to hide "could not get response" warning. Alternatively to show them in the first place
//TODO: Add iqdb subcommand where local image specified gets posted to iqdb and a larger image is received.
//TODO: To increase speed search for new links if an image has not been found or does not work. (Use objects which has a 'call next link' method)
//TODO: Add progress bar, like when compiling with cargo
//TODO: add renaming subcommand where folder name & name in threads.txt are updated
static DEBUG: AtomicBool = AtomicBool::new(false);
static PRINT_NUMBERED: AtomicBool = AtomicBool::new(true);

const USER_AGENT: &str = "user-agent";
const USER_AGENT_VALUE: &str = "Mozilla/5.0 (X11; Linux x86_64; rv:87.0) Gecko/20100101 Firefox/87.0";


fn main() {
    let matches = cli::build_cli().get_matches();
    let mut update_modify_date: bool = false;

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
        ("update", Some(args)) => {
            threads.retain(|thread| {
                debug_output("update url", &thread.0);
                let res = chan(&thread.0, 
                    update_modify_date, 
                    Some(&thread.1), 
                    None,
                    // iqdb is not supported with update
                    false,
                    false,
                    args.is_present("print-existing-images")
                    
                );
                // Removes thread from file if chan returns None, which means that the thread has been archived.
                // Keeps element if chan returns Some
                if res.is_none() {
                    false
                }
                else {
                    true
                }
            });
            debug_output("saving", "Updating threads.txt file");
            vec_to_file(threads);
        },
        ("download", Some(args)) => {
            let url: String = args.value_of("url").expect("No url provided").to_string();

            let thread = chan(&url, 
                update_modify_date, 
                args.value_of("directory"), 
                args.value_of("name"), 
                args.is_present("iqdb"),
                args.is_present("override"),
                true
            ).unwrap();

            if ! args.is_present("iqdb") {
                // Add link to threads file for 'update' subcommand if not present
                threads.push(thread.clone());
                threads.dedup();

                // Saves threads after chan() call to avoid non-working links
                debug_output("saving", "Saving url to file");
                vec_to_file(threads);
            }
        }
        _ => println!("No Subcommands; how is this possible?"),
    }
}

/// The procedure of grabbing information to downloading the images from the thread. Returns None if thread has been archived
fn chan<S: AsRef<str>>(
        url: S, 
        update_modify_date: bool,
        param_dir: Option<&str>,
        param_name: Option<&str>,
        iqdb: bool,
        override_enabled: bool,
        print_existing_images: bool,
) -> Option<(String, String)> {
    let url: String = url.as_ref().to_string();
    let dir: String;
    let dir_path: PathBuf;
    let mut number: u64 = 0;
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
    else if param_name.is_some() {
        let thread_id = match get_name(&url, true) {
            Ok(t) => t,
            Err(r) => {
                if r.is_none() {
                    println!("{} Could not get a response from {}", Red.paint("Error:"), &url);
                }
                else if r.as_ref().unwrap().status() == 404 {
                    println!("{} Thread {} could not be found, site returned 404 status error", Red.paint("Error:"), &url);
                }
                else {
                    println!("{} Response error {} received from thread {}", Red.paint("Error:"), r.unwrap().status(), &url)
                }
                process::exit(1);
            }
        };
        dir = format!("{} - {}", thread_id, param_name.unwrap());
    }
    else {
        dir = match get_name(&url, false) {
            Ok(t) => t,
            //TODO: keep code DRY, this block is identical to the one above. Create function?
            Err(r) => {
                if r.is_none() {
                    println!("{} Could not get a response from {}", Red.paint("Error:"), &url);
                }
                else if r.as_ref().unwrap().status() == 404 {
                    println!("{} Thread {} could not be found, site returned 404 status error", Red.paint("Error:"), &url);
                }
                else {
                    println!("{} Response error {} received from thread {}", Red.paint("Error:"), r.unwrap().status(), &url)
                }
                process::exit(1);
            }
        };
    }
    dir_path = Path::new(".").join(&dir);
    
    println!("Downloading images to {}/", Cyan.paint(&dir));

    // dumps thumbnails image links on site to 'urls' to use with iqdb
    urls = match get_links(&url) { 
        Ok(links) => {
            if iqdb {
                links.into_iter()
                // Grab only thumbnail images
                .filter(|n| (
                        n.ends_with(".jpg") || 
                        n.ends_with(".gif") || 
                        n.ends_with(".png") || 
                        n.ends_with(".jpeg") || 
                        n.ends_with(".webm")
                    ) &&
                    !n.contains("url=") &&
                    n.contains("/thumb/"))
                    // Split at http to separate the two links and get the second one
                    .map(|n|
                        format!("http{}", n.split("http")
                        .filter(|&s| !s.is_empty())
                        .collect::<Vec<_>>()
                        .last().unwrap().to_string()))
                        .collect()
            }
            else {
                links.into_iter()
                .filter(|n| (
                        n.ends_with(".jpg") || 
                        n.ends_with(".gif") || 
                        n.ends_with(".png") || 
                        n.ends_with(".jpeg") || 
                        n.ends_with(".webm") 
                    ) &&
                    !n.contains("url=") &&
                    // Filter away all thumbnail images and only keep the hi-res ones
                    !n.contains("/thumb/"))
                    .map(|n| 
                        if ! n.starts_with("http") { 
                            n.replace("//", "https://") 
                        } 
                        else { 
                            n 
                        })
                    .collect()
            }
        },
        Err(r) => {
            if r.unwrap().status() == 404 {
                println!("Thread {} has been archived, removing from file", &url);
            }
            return None
        }
    };

    urls.dedup();
    
    debug_output("urls_vec", format!("{:#?}", urls).as_str());

    // Create directory if it does not exist
    if ! dir_path.is_dir() {
        create_dir(&dir).expect("Could not create directory, may not have write permission");
    }

    for img in urls.iter() {
        // Path for new file
        let file_path: Option<PathBuf>;

        number += 1;
        
        // Downloads file
        file_path = download(
            &dir_path,
            &dir, 
            &img, 
            img_links.clone(), 
            override_enabled,
            iqdb,
            print_existing_images,
            number
        );

        if update_modify_date && file_path.is_some() {
            let file_path = file_path.unwrap();
            // Timestamp to assign to file
            // By using the number variable and checked_add() method ensures that all files are at least 1 second apart to ensure correct order in file-managers
            let new_timestamp: FileTime = FileTime::from_system_time(SystemTime::now().checked_add(time::Duration::from_secs(number)).unwrap());
            debug_output("file path", file_path.to_str().unwrap());
            debug_output("new_timestamp", new_timestamp.to_string().as_str());
            set_file_mtime(file_path, new_timestamp).expect("Could not update modified date");
        }
        
    }

    return Some((url, dir));
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
            iqdb: bool,
            print_existing_images: bool,
            number: u64,
    ) -> Option<PathBuf> {

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
    file_path = dir_path.as_ref().join(&name);

    //TODO: (Should this be moved to chan() instead?)
    // This block is used with --iqdb flag and gathers all image links from all links that were scraped from the image search 
    if iqdb && ( !file_path.is_file() || override_enabled ) {
        // Get name without extension or 's' for thumbnails
        name = name.replace("s", "");
        file_name = name.split(".").collect::<Vec<_>>()[0].to_string();
        file_path = dir_path.as_ref().join(name.as_str());

        // Check if a file with the same name exists (ignores file extension)
        let files = read_dir(&dir_path).expect("Could not read directory");
        let mut exists = false;
        for file in files {
            let file = file.unwrap();
            if file.path().to_str().unwrap().contains(&file_name) && ! override_enabled {
                    exists = true;
                    // Updates name with the correct extension
                    name = file.path().file_name().unwrap().to_str().unwrap().to_string();
                    file_path = file.path();
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
            iqdb_link = format!("https://iqdb.org/?url={}", img.as_ref());
            debug_output("iqdb_link", &iqdb_link);
            
            //BUG: Must handle when get_links returns None
            // Lists all links on site and removes non useful links
            let mut iqdb_urls: Vec<String> = get_links( &iqdb_link).unwrap()
                            // Get all links before the '#' link (since all after are irrelevant)
                            .split(|n| n == &"#".to_string())
                            .collect::<Vec<_>>()[0].to_vec()
                            // Format links
                            .into_iter()
                            // Remove first element ('/' link)
                            .filter(|n| n != "/")
                            // Conditioned because sometimes links already starts with "https://"
                            .map(|n| if n.starts_with("//") { n.replace("//", "https://") } else { n } )
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
                let mut new_imgs = match get_links(url) {
                    Ok(links) => {
                        links.into_iter()
                            .filter(|n| (
                                n.ends_with(".jpg") || 
                                n.ends_with(".gif") || 
                                n.ends_with(".png") || 
                                n.ends_with(".jpeg") || 
                                n.ends_with(".webm") ) &&
                                !n.contains("url="))
                            .collect::<Vec<_>>()
                    },
                    Err(r) => {
                        debug_output("Response error", format!("{} returned {:?}", url, r).as_str());
                        Vec::new()
                    }
                };
                debug_output("new imgs", &format!("{:#?}", new_imgs));

                // In case of error when new_imgs is empty
                if ! new_imgs.is_empty() {
                    img_links.append(&mut new_imgs);
                }
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
        if print_existing_images {
            if PRINT_NUMBERED.load(Ordering::Relaxed) {
                print!("[{}] ", Blue.paint(number.to_string()));
            }
            println!("{} {} in {}", 
                name.as_str(), 
                Blue.paint("already exists"), 
                dir.as_ref());
        }
        else {
            debug_output("exists", name.as_str());
        }
    }
    else if iqdb_not_found {
        if PRINT_NUMBERED.load(Ordering::Relaxed) {
            print!("[{}] ", Blue.paint(number.to_string()));
        }
        println!("{} on iqdb.org\n\t{}", 
        Red.paint("Image not found"),
        &iqdb_link);
        return None;
    }
    else if iqdb_no_image_link_found {
        if PRINT_NUMBERED.load(Ordering::Relaxed) {
            print!("[{}] ", Blue.paint(number.to_string()));
        }
        println!("Image found on iqdb.org but {}\n\t{}", 
                Yellow.paint("can not be downloaded automatically"), 
                &iqdb_link);
        return None;
    }
    else {
        if PRINT_NUMBERED.load(Ordering::Relaxed) {
            print!("[{}] ", Blue.paint(number.to_string()));
        }
        print!("Downloading {} to {} ", name.as_str(), dir.as_ref());

        if DEBUG.load(Ordering::Relaxed) {
            println!("");
        }
        
        // Iterate over found image urls until a with data is produced
        // BUG: Download from chan.sankakucomplex.com
        // TODO: Give error if no link works, check if break is called in for loop!
        for url in img_links.iter() {
            let extension = url.split(".").last().expect("No extension found");
            debug_output("extension", extension);
            file_path.set_extension(extension);

            debug_output("Trying", url.as_str());

            let mut resp: Response = match get_response(&url) {
                Some(r) => r,
                None => {
                    println!("Could not get a response from {}, continuing", &url);
                    continue
                },
            };

            debug_output("name", &file_path.as_os_str().to_str().unwrap());

            let tmpfile_named: NamedTempFile = tempfile::NamedTempFile::new().unwrap();
            // let mut tmpfile: File = tmpfile_named.reopen().unwrap();
            let mut tmpfile: &File = tmpfile_named.as_file();
            debug_output("tmp_file", &format!("{:?}", &tmpfile));
            
            // let mut file: File = File::create(&file_path.as_os_str()).expect("Could not create file");
            // std::io::copy(&mut resp, &mut file).expect("Could not download image to file");
            std::io::copy(&mut resp, &mut tmpfile).expect("Could not download image to file");
            copy(tmpfile_named.path(), &file_path).expect("Could not copy file, aborting");
            tmpfile_named.close().expect("Could not delete temporary file");

            let size = std::fs::metadata(&file_path).unwrap().len();
            // Stupid solution where image must be larger than 1 kB as not to download a 404 page or something as an image
            // TODO: fix this, possible to check if image is valid?
            debug_output("size", &size.to_string());
            // Break if downloaded file contains data
            if file_path.exists() && size > 1000 {
                break;
            }
            // TODO: Remove old file less than 1000 bytes here, since it will stay if next image has another file extension
        }

        println!("{}", Green.paint("Done"));
    }
    return Some(file_path);
}

/// Returns all lines in a file as a Vector
fn file_to_vec() -> Result<Vec<(String, String)>, String> {
    let res: Vec<(String, String)>;
    let contents;
    contents = match read_to_string("threads.txt") {
        Ok(contents) => contents,
        Err(_) => {
            println!("threads.txt does not exist, creating {}", Green.paint("Done"));
            std::fs::write("threads.txt", "").expect("Could not create threads.txt file");
            String::new()
        }
    };
    res = contents
                // Remove whitespace and special characters
                .split("\n").map(|s| s.trim().to_string()).filter(|s| ! s.is_empty())
                // Convert format to tuple
                .map(|s| {
                    let t: (&str, &str) = s.split_once(";").expect("Could not split String");
                    // Convert (&str, &str) to (String, String)
                    (t.0.to_string(), t.1.to_string())
                }).collect();
    return Ok(res);
}

/// Creates `Response` object from given url. `None` if no response were given from site.
fn get_response(url: &str) -> Option<Response> {
    let client: Client;

    if url.contains("iqdb.org") {
        // Ignore timeout for iqdb since it can take a while without being an error
        client = Client::builder().build().unwrap();
    }
    else {
        client = Client::builder().timeout(time::Duration::from_secs(5)).build().unwrap();
    }

    let resp: Response;
    let mut i = 0;

    loop {
        resp = match client.get(url).header(USER_AGENT, USER_AGENT_VALUE).send() {
            Ok(r) => r,
            Err(_) => {
                //TODO: is this block really necessary if a timeout is set?
                i += 1;
                if i == 2 {
                    println!("{} Could not get a response from {}, continuing", Yellow.paint("Warning:"), url);
                    return None
                }
                println!("Could not get response, retrying...");
                thread::sleep(time::Duration::from_secs(1));
                continue
            }
        };
        // If this is reached, then match returned Ok()
        break
    }
    Some(resp)
}

// BUG: Handle redirects in loop to get to pointed site. (for archived.moe which redirects to other sites)
/// Returns HTML Document of given site. 
/// Error contains `Response` if it could be retrieved, otherwise `None`
fn get_html(url: &str) -> Result<Document, Option<Response>> {
    
    let mut resp: Response = match get_response(&url) {
        Some(r) => r,
        None => return Err(None),
    };

    if !resp.status().is_success() {
        debug_output("status error on", url);
        return Err(Some(resp))
    }

    let document = match Document::from_read(&mut resp) {
        Ok(d) => d,
        Err(e) => {
            println!("{:#?}", resp); 
            println!("{:#?}", e); 
            return Err(None)
        },
    };

    return Ok(document);
}

/// Returns Vector with all links found in anchor tags on given site
/// Error contains `Response` if it could be retrieved, otherwise `None`
fn get_links(url: &str) -> Result<Vec<String>, Option<Response>> {
    let mut res: Vec<String> = Vec::new();

    match get_html(url) {
        Ok(d) => { d.find(Name("a"))
                            .filter_map(|n| n.attr("href"))
                            .for_each(|n| res.push(n.to_string()))
            },
        Err(r) => return Err(r)
    }
    return Ok(res)
}

/// Returns a folder name with the "{thread-id} - {thread subject}" pattern
/// Error contains `Response` if it could be retrieved, otherwise `None`
fn get_name<S: AsRef<str>>(url: S, only_thread_number: bool) -> Result<String, Option<Response>> {
    debug_output("get_name url", url.as_ref());
    let doc = match get_html(&url.as_ref()) {
        Ok(d) => d,
        Err(r) => return Err(r)
    };
        let thread_number: String = url.as_ref().split("/").filter(|&s| !s.is_empty()).collect::<Vec<_>>().last().unwrap().to_string();
        let mut subject_node = doc.find(Class("subject")).collect::<Vec<_>>();

        if subject_node.first().is_none() || subject_node.first().unwrap().text().is_empty() {
            subject_node = doc.find(Class("name")).collect::<Vec<_>>();
        }
        if subject_node.first().is_none() || subject_node.first().unwrap().text().is_empty() {
            // Class on archived.moe
            subject_node = doc.find(Class("post_title")).collect::<Vec<_>>();
        }

        let subject: String;
        if subject_node.first().is_none() {
            subject = "title".to_string();
        }
        else {
            subject = subject_node.first().unwrap().text().replace("/", " ");
        }

        if only_thread_number {
            return Ok(format!("{}", thread_number))
        }
        else {
            return Ok(format!("{} - {}", thread_number, subject));
        }
}

//BUG: When saving to file that has been updated those updates are lost. Read from file first and include the new records
/// Removes existing file and writes links from Vec with the format (String, String) to it
fn vec_to_file(vec: Vec<(String, String)>) {
    // std::fs::remove_file("threads.txt");
    let mut file = File::create("threads.txt").expect("Could not create file");

	for l in vec.iter() {
		file.write(format!("{};{}\n", l.0, l.1).as_bytes()).expect("Could not write to file");
	}
}
