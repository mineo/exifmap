extern crate failure;
extern crate geojson;
extern crate gexiv2_sys;
extern crate libc;
#[macro_use]
extern crate log;
extern crate env_logger;
extern crate magick_rust;
extern crate rayon;
extern crate rexiv2;
extern crate serde_json;
extern crate walkdir;

use geojson::{FeatureCollection, Feature, Geometry, Value};
use libc::size_t;
use magick_rust::{MagickWand, magick_wand_genesis};
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
    path: path::PathBuf,
    thumbnail_filename: String,
    gpsinfo: rexiv2::GpsInfo,
}

impl MediaInfo {
    pub fn new(path: path::PathBuf, gpsinfo: rexiv2::GpsInfo) -> MediaInfo {
        MediaInfo {
            thumbnail_filename: MediaInfo::thumbnail_filename(path.as_ref()),
            path: path,
            gpsinfo: gpsinfo,
        }
    }


    pub fn from_path(path: path::PathBuf) -> EMResult<MediaInfo> {
        let metadata = rexiv2::Metadata::new_from_path(&path)?;
        match metadata.get_gps_info() {
            None => Err(EMError::NoGPSInformation{ filename: path.to_string_lossy().to_string()})?,
            Some(gpsinfo) => Ok(MediaInfo::new(path, gpsinfo)),
        }
    }

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

    pub fn generate_thumbnail(&self, target_directory: &path::Path, width: size_t, height: size_t) -> EMResult<()> {
        let wand = MagickWand::new();
        if let Err(e) = wand.read_image(self.path.to_str().unwrap()) {
            failure::bail!("Error reading: {}", e);
        }
        wand.fit(width, height);
        let mut complete_thumbnail_filename = target_directory.to_owned();
        complete_thumbnail_filename.push(self.thumbnail_filename.to_owned());
        if complete_thumbnail_filename.exists() {
            failure::bail!("{} exists!", complete_thumbnail_filename.display());
        }
        if let Err(e) = wand.write_image(complete_thumbnail_filename.to_str().unwrap()) {
            failure::bail!("Error writing: {}", e);
        }
        Ok(())
    }

    fn thumbnail_filename(path: &path::Path) -> String {
        let original_file_stem = path.file_stem().expect(&format!("MediaInfo without filename: {}", path.display())).to_str().unwrap();
        let original_file_extension = path.extension().expect(&format!("MediaInfo without file extension: {}", path.display())).to_str().unwrap();
        format!("{}_thumb.{}", original_file_stem, original_file_extension)
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
            let path = entry.unwrap().into_path();
            MediaInfo::from_path(path)
        })
        .collect()
}

fn main() -> EMResult<()> {
    static START: Once = Once::new();

    START.call_once(|| {
        unsafe {
            gexiv2_sys::gexiv2_initialize();
            magick_wand_genesis();
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

    assert!(outpath.is_dir());

    let features: Vec<Feature> = mediainfos_from_dir(indir)
        .into_par_iter()
        .map(|maybemediainfo| maybemediainfo.map_err(|e| {
            match e.as_fail().downcast_ref::<EMError>() {
                Some(EMError::NoGPSInformation { .. }) => trace!("{}", e),
                _ => error!("{}", e),
            }
        }))
        .flatten()
        .map(|m| {
            match m.generate_thumbnail(outpath.as_ref(), 500, 500) {
                Err(e) => Err(e),
                _ => Ok(m)
            }
        })
        .map(|maybemediainfo| maybemediainfo.map_err(|e| error!("{}", e)))
        .flatten()
        .map(|m| m.to_feature())
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
