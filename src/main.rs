extern crate failure;
extern crate geojson;
#[macro_use]
extern crate log;
extern crate env_logger;
extern crate rexiv2;
extern crate serde_json;
extern crate walkdir;

use geojson::{FeatureCollection, Feature, Geometry, Value};
use serde_json::{Map, to_value};
use std::convert::From;
use std::fs;
use std::path;
use std::env;
use walkdir::WalkDir;

type EMResult<T> = std::result::Result<T, failure::Error>;

fn get_gps_info(path: &path::PathBuf) -> EMResult<Option<rexiv2::GpsInfo>> {
    let metadata = rexiv2::Metadata::new_from_path(path)?;
    match metadata.get_gps_info() {
        None => Ok(None),
        Some(gpsinfo) => Ok(Some(gpsinfo)),
    }
}

fn feature_from_gpsinfo(gpsinfo: &rexiv2::GpsInfo, path: &str) -> EMResult<Feature> {
    info!("{}", path);
    info!("{:?}", gpsinfo);
    let mut properties = Map::new();
    properties.insert(String::from("filename"), to_value(path)?);
    let thispoint = Value::Point(vec![gpsinfo.longitude, gpsinfo.latitude]);
    let thisgeometry = Geometry::new(thispoint);
    let thisfeature = Feature {
        bbox: None,
        geometry: Some(thisgeometry),
        id: None,
        properties: Some(properties),
        foreign_members: None,
    };
    Ok(thisfeature)
}

fn featurecollection_from_dir(dirname: &str) -> EMResult<FeatureCollection> {
    let features: Vec<Feature> = WalkDir::new(dirname)
        .into_iter()
        .filter(|e| match e {
            // TODO: This is similar to map_or_else on nightly
            Ok(f) => f.file_type().is_file(),
            Err(err) => {
                error!("{:?}: {}", e, err);
                false
            }
        })
        // TODO: filter Err entries here, but log them
        .map(|entry| {
            let path = entry.unwrap().path().to_owned();
            let gpsinfo = get_gps_info(&path);
            (path.display().to_string().to_owned(), gpsinfo)
        })
        .filter_map(|(path, gpsinfo)| {
            match gpsinfo {
                Ok(None) => {
                    info!("{}: No GPS info", path);
                    None
                }
                Err(e) => {
                    error!("{}: {}", path, e);
                    None
                }
                Ok(Some(gps)) => Some((path, gps))
            }}
        )
        .filter_map(|(path, gpsinfo)| {
            match feature_from_gpsinfo(&gpsinfo, &path) {
                Ok(feature) => Some(feature),
                Err(e) => {
                    error!("{}: {}", path, e);
                    None
                }
            }
        })
        .collect();
    let allfeatures = FeatureCollection {
        bbox: None,
        features: features,
        foreign_members: None,
    };
    Ok(allfeatures)
}

fn main() -> Result<(), failure::Error> {
    env_logger::init();
    let args: Vec<String> = env::args().collect();
    let indir = args.get(1).expect("No directory name");
    let outfile = args.get(2).expect("No output file name");
    featurecollection_from_dir(indir)
        .and_then(|f| serde_json::to_string(&f).map_err(From::from))
        .and_then(|s| fs::write(outfile, s).map_err(From::from))
}
