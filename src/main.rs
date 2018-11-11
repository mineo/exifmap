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

fn get_gps_info(path: &path::Path) -> EMResult<Option<rexiv2::GpsInfo>> {
    let metadata = rexiv2::Metadata::new_from_path(path)?;
    match metadata.get_gps_info() {
        None => Ok(None),
        Some(gpsinfo) => Ok(Some(gpsinfo)),
    }
}

fn feature_from_gpsinfo(gpsinfo: &rexiv2::GpsInfo, path: &path::Path) -> EMResult<Feature> {
    info!("{}", path.display());
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
    let mut features: Vec<Feature> = vec![];
    for entry in WalkDir::new(dirname).into_iter().filter(|e| match e {
        // TODO: This is similar to map_or_else on nightly
        Ok(f) => f.file_type().is_file(),
        Err(err) => {
            error!("{:?}: {}", e, err);
            false
        }
    })
    {
        let eu = entry.unwrap();
        let path = eu.path();
        let nicepath = path.display();
        match get_gps_info(path) {
            Ok(Some(gpsinfo)) => {
                match feature_from_gpsinfo(&gpsinfo, path) {
                    Ok(feature) => features.push(feature),
                    Err(e) => error!("{}: {}", nicepath, e),
                }
            }
            Ok(None) => info!("{}: No GPS info", nicepath),
            Err(e) => error!("{}: {}", nicepath, e),
        }
    }
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
