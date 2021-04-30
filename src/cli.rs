use clap::{App, AppSettings, Arg, SubCommand};

pub fn build_cli() -> App<'static, 'static> {
    App::new("4Chan Image Downloader")
            // Displays friendly help message if no argument is given
            .setting(AppSettings::ArgRequiredElseHelp)
            .version("0.1")
            // .author("Esbjörn S. <me@stagrim.com>")
            .about("Download 4chan images")
            // Convert to subcommand?
            .arg(Arg::with_name("iqdb")
                .short("i")
                .long("iqdb")
                .help("Gather hi-res images from iqdb.org from archive sites"))
            .arg(Arg::with_name("directory")
                .short("d")
                .long("dir")
                .value_name("DIRECTORY")
                .takes_value(true)
                .help("Save files to <DIRECTORY>"))
            .arg(Arg::with_name("override")
                .short("o")
                .long("override")
                .help("Override existing files"))
            .arg(Arg::with_name("not-numbered")
                .long("not-numbered")
                .short("n")
                .help("Do not print image number in output"))
            //TODO: Not yet implemented
            .arg(Arg::with_name("quiet")
                .short("q")
                .long("quiet")
                .help("Disables output"))
            .arg(Arg::with_name("debug")
                .short("D")
                .long("debug")
                .help("Enables debug output"))
            .arg(Arg::with_name("update-modify-date")
                .short("u")
                .long("update-modify-date")
                .help("Updates modify date of existing images")
                .long_help(
                    "Updates the modify date to the current time when downloading images.\nThis ensures that images will be in order of time posted when sorted by modification date"))
            .arg(Arg::with_name("url")
                .help("Link to 4chan thread")
                .required(true))
            .subcommand(SubCommand::with_name("update")
                .about("Updates downloaded threads using the threads.txt file in current directory"))
}