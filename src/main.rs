extern crate failure;
extern crate geojson;
extern crate gexiv2_sys;
#[macro_use]
extern crate log;
extern crate env_logger;
extern crate rayon;
extern crate rexiv2;
extern crate serde_json;
extern crate walkdir;

use geojson::{FeatureCollection, Feature, Geometry, Value};
use rayon::prelude::*;
use serde_json::{Map, to_value};
use std::convert::From;
use std::fs;
use std::path;
use std::env;
use std::sync::Once;
use walkdir::WalkDir;

type EMResult<T> = std::result::Result<T, failure::Error>;

#[derive(Debug, failure::Fail)]
enum EMError {
    #[fail(display = "{} has no GPS information", filename)]
    NoGPSInformation { filename: String },
    #[fail(display = "Input path '{}' contains output path '{}'", inpath, outpath)]
    InpPathContainsOutPath { inpath: String, outpath: String },
}

struct MediaInfo {
    path: String,
    gpsinfo: rexiv2::GpsInfo,
}

impl MediaInfo {
    pub fn to_feature(&self) -> EMResult<Feature> {
        let mut properties = Map::new();
        properties.insert(String::from("filename"), to_value(self.path.to_owned())?);

        let thispoint = Value::Point(vec![
            self.gpsinfo.longitude,
            self.gpsinfo.latitude,
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

fn mediainfos_from_dir(dirname: &str) -> Vec<EMResult<MediaInfo>> {
    WalkDir::new(dirname)
        .into_iter()
        .par_bridge()
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
            match get_gps_info(&path) {
                Err(e) => Err(e),
                Ok(Some(gps)) =>
                    Ok(MediaInfo {
                        path: path.display().to_string().to_owned(),
                        gpsinfo: gps,
                    }),
                Ok(None) =>
                    Err(EMError::NoGPSInformation{ filename: path.to_string_lossy().to_string() })?,
            }
        })
        .collect()
}

fn main() -> EMResult<()> {
    static START: Once = Once::new();

    START.call_once(|| {
        unsafe {
            gexiv2_sys::gexiv2_initialize();
        }
    });

    env_logger::init();
    let args: Vec<String> = env::args().collect();
    let indir = args.get(1).expect("No directory name");
    let inpath = path::PathBuf::from(indir).canonicalize()?;
    let outdir = args.get(2).expect("No output directory name");
    let outpath = path::PathBuf::from(outdir).canonicalize()?;
    let mut outfile = outpath.clone();
    outfile.push("data.json");

    if outpath.as_path().starts_with(inpath.as_path()) {
        return Err(EMError::InpPathContainsOutPath{
            inpath: String::from(inpath.to_string_lossy()),
            outpath: String::from(outpath.to_string_lossy())
        })?;
    }

    let features = mediainfos_from_dir(indir)
        .into_par_iter()
        .flat_map(|m| m.map(|i| i.to_feature()))
        .map(|maybemediainfo| maybemediainfo.map_err(|e| error!("{}", e)))
        .flatten()
        .collect();
    let allfeatures = FeatureCollection {
        bbox: None,
        features: features,
        foreign_members: None,
    };
    Ok(allfeatures)
        .and_then(|f| serde_json::to_string(&f).map_err(From::from))
        .and_then(|s| fs::write(outfile, s).map_err(From::from))
}
