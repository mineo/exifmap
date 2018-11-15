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

#[derive(Debug, failure::Fail)]
enum EMError {
    #[fail(display = "{} has no GPS information", filename)]
    NoGPSInformation { filename: String },
}

struct MediaInfo {
    path: String,
    gpsinfo: Option<rexiv2::GpsInfo>,
}

impl MediaInfo {
    pub fn to_feature(&self) -> EMResult<Feature> {
        if self.gpsinfo.is_none() {
            return Err(EMError::NoGPSInformation{ filename: self.path.to_owned() })?;
        };

        let mut properties = Map::new();
        properties.insert(String::from("filename"), to_value(self.path.to_owned())?);

        let thispoint = Value::Point(vec![
            self.gpsinfo.unwrap().longitude,
            self.gpsinfo.unwrap().latitude,
        ]);
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
}

fn get_gps_info(path: &path::PathBuf) -> EMResult<Option<rexiv2::GpsInfo>> {
    let metadata = rexiv2::Metadata::new_from_path(path)?;
    match metadata.get_gps_info() {
        None => Ok(None),
        Some(gpsinfo) => Ok(Some(gpsinfo)),
    }
}

fn featurecollection_from_dir(dirname: &str) -> EMResult<FeatureCollection> {
    let features: Vec<_> = WalkDir::new(dirname)
        .into_iter()
        .filter(|e| match e {
            // TODO: This is similar to map_or_else on nightly
            Ok(f) => f.file_type().is_file(),
            Err(err) => {
                error!("{:?}: {}", e, err);
                false
            }
        })
        .map(|entry| {
            let path = entry.unwrap().path().to_owned();
            get_gps_info(&path).map(|gps| {
                MediaInfo {
                    path: path.display().to_string().to_owned(),
                    gpsinfo: gps,
                }
            })
        })
        .flat_map(|m| m.map(|i| i.to_feature()))
        .map(|converted| converted.map_err(|e| error!("{}", e)))
        .flatten()
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
