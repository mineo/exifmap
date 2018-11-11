extern crate failure;
#[macro_use]
extern crate log;
extern crate rexiv2;
extern crate simple_logger;
extern crate walkdir;

use std::path;
use std::env;
use walkdir::WalkDir;

type EMResult<T> = std::result::Result<T, failure::Error>;

fn get_gps_info(path: &path::Path) -> EMResult<Option<rexiv2::GpsInfo>> {
    let metadata = rexiv2::Metadata::new_from_path(path)?;
    match metadata.get_gps_info() {
        None => Ok(None),
        Some(gpsinfo) => Ok(Some(gpsinfo)),
    }
}

fn walk_files_in_dir(dirname: &str) {
    for entry in WalkDir::new(dirname) {
        let eu = entry.unwrap();
        let path = eu.path();
        match get_gps_info(path) {
            Ok(Some(gpsinfo)) => {
                info!("{}", path.display());
                info!("{:?}", gpsinfo);
            }
            Ok(None) => info!("No GPS info in {}", path.display()),
            Err(e) => error!("{}", e),
        }
    }
}

fn main() {
    match simple_logger::init_with_level(log::Level::Info) {
        Err(e) => {
            println!("Failed to intialize logging: {}", e);
            return;
        }
        Ok(()) => {}

    }
    let args: Vec<String> = env::args().collect();
    match args.get(1) {
        Some(dirname) => walk_files_in_dir(dirname),
        None => error!("No directory name"),
    }
}
