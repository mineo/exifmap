extern crate failure;
extern crate rexiv2;
extern crate walkdir;

use std::path;
use std::env;
use walkdir::WalkDir;

type EMResult<T> = std::result::Result<T, failure::Error>;

fn get_gps_info(path: &path::Path) -> EMResult<rexiv2::GpsInfo> {
    let metadata = rexiv2::Metadata::new_from_path(path)?;
    match metadata.get_gps_info() {
        None => failure::bail!("No GPS info in {}", path.display()),
        Some(gpsinfo) => Ok(gpsinfo),
    }
}

fn walk_files_in_dir(dirname: &str) {
    for entry in WalkDir::new(dirname) {
        let eu = entry.unwrap();
        let path = eu.path();
        match get_gps_info(path) {
            Ok(gpsinfo) => {
                println!("{}", path.display());
                println!("{:?}", gpsinfo);
            }
            Err(e) => println!("{}", e),
        }
    }
}

fn main() {
    let args: Vec<String> = env::args().collect();
    match args.get(1) {
        Some(dirname) => walk_files_in_dir(dirname),
        None => println!("No directory name"),
    }
}
