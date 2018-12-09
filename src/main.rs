use clap::{App, Arg};
use geojson::{Feature, FeatureCollection, Geometry, Value};
use libc::size_t;
use log::{error, info, trace};
use magick_rust::{magick_wand_genesis, MagickWand};
use rayon::prelude::*;
use serde_json::{to_value, Map};
use std::convert::From;
use std::fs;
use std::path;
use std::sync::Once;
use walkdir::WalkDir;

type EMResult<T> = std::result::Result<T, failure::Error>;

#[derive(Debug, failure::Fail)]
enum EMError {
    #[fail(display = "{} has no GPS information", filename)]
    NoGPSInformation { filename: String },
    #[fail(display = "Unable to losslessly deal with filename '{:?}' ", filename)]
    NoLosslessProcessingPossible { filename: path::PathBuf },
}

struct MediaInfo {
    path: path::PathBuf,
    thumbnail_filename: String,
    gpsinfo: rexiv2::GpsInfo,
}

impl MediaInfo {
    pub fn new(path: path::PathBuf, gpsinfo: rexiv2::GpsInfo) -> EMResult<MediaInfo> {
        let thumbnail_filename = MediaInfo::generate_thumbnail_filename(path.as_ref())?;
        Ok(MediaInfo {
            thumbnail_filename: thumbnail_filename,
            path: path,
            gpsinfo: gpsinfo,
        })
    }

    pub fn from_path(path: path::PathBuf) -> EMResult<MediaInfo> {
        let metadata = rexiv2::Metadata::new_from_path(&path)?;
        match metadata.get_gps_info() {
            None => Err(EMError::NoGPSInformation {
                filename: path.to_string_lossy().to_string(),
            })?,
            Some(gpsinfo) => MediaInfo::new(path, gpsinfo),
        }
    }

    pub fn to_feature(&self) -> EMResult<Feature> {
        let mut properties = Map::new();
        properties.insert(String::from("filename"), to_value(self.path.to_owned())?);
        properties.insert(
            String::from("thumbnail_filename"),
            to_value(self.thumbnail_filename.to_owned())?,
        );

        let thispoint = Value::Point(vec![self.gpsinfo.longitude, self.gpsinfo.latitude]);
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

    pub fn generate_thumbnail(
        &self,
        target_directory: &path::Path,
        width: size_t,
        height: size_t,
    ) -> EMResult<()> {
        let wand = MagickWand::new();
        if let Err(e) = wand.read_image(self.path.to_str().unwrap()) {
            failure::bail!("Error reading '{}': {}", self.path.display(), e);
        }
        wand.fit(width, height);
        let mut complete_thumbnail_filename = target_directory.to_owned();
        complete_thumbnail_filename.push(self.thumbnail_filename.to_owned());
        if complete_thumbnail_filename.exists() {
            failure::bail!("{} exists!", complete_thumbnail_filename.display());
        }
        if let Err(e) = wand.write_image(complete_thumbnail_filename.to_str().unwrap()) {
            failure::bail!(
                "Error writing '{}': {}",
                complete_thumbnail_filename.display(),
                e
            );
        }
        Ok(())
    }

    fn generate_thumbnail_filename(path: &path::Path) -> EMResult<String> {
        let original_file_stem = path
            .file_stem()
            .expect(&format!("MediaInfo without filename: {}", path.display()))
            .to_str()
            .ok_or_else(|| EMError::NoLosslessProcessingPossible {
                filename: path.to_owned(),
            })?;
        let original_file_extension = path
            .extension()
            .expect(&format!(
                "MediaInfo without file extension: {}",
                path.display()
            ))
            .to_str()
            .ok_or_else(|| EMError::NoLosslessProcessingPossible {
                filename: path.to_owned(),
            })?;
        Ok(format!(
            "{}_thumb.{}",
            original_file_stem, original_file_extension
        ))
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

    START.call_once(|| unsafe {
        gexiv2_sys::gexiv2_initialize();
        magick_wand_genesis();
    });

    env_logger::init();

    let matches = App::new("Exifmap")
        .version("0.1")
        .author("Wieland Hoffmann")
        .arg(Arg::with_name("indir").value_name("INDIR").required(true))
        .arg(Arg::with_name("outdir").value_name("OUTDIR").required(true))
        .get_matches();
    let indir = matches.value_of("indir").unwrap();
    let inpath = path::PathBuf::from(indir).canonicalize()?;
    let outdir = matches.value_of("outdir").unwrap();
    let outpath = path::PathBuf::from(outdir).canonicalize()?;
    let mut outfile = outpath.clone();
    outfile.push("data.json");

    if outpath.as_path().starts_with(inpath.as_path()) {
        failure::bail!("'{}' contains '{}'", inpath.display(), outpath.display());
    }

    assert!(outpath.is_dir());

    let features: Vec<Feature> = mediainfos_from_dir(indir)
        .into_par_iter()
        .map(|maybemediainfo| {
            maybemediainfo.map_err(|e| match e.as_fail().downcast_ref::<EMError>() {
                Some(EMError::NoGPSInformation { .. }) => trace!("{}", e),
                _ => error!("{}", e),
            })
        })
        .flatten()
        .map(|m| match m.generate_thumbnail(outpath.as_ref(), 500, 500) {
            Err(e) => Err(e),
            _ => Ok(m),
        })
        .map(|maybemediainfo| maybemediainfo.map_err(|e| error!("{}", e)))
        .flatten()
        .map(|m| m.to_feature())
        .flatten()
        .collect();

    info!(
        "Wrote {} thumbnails to '{}'",
        features.len(),
        outpath.display()
    );

    let allfeatures = FeatureCollection {
        bbox: None,
        features: features,
        foreign_members: None,
    };

    Ok(allfeatures)
        .and_then(|f| serde_json::to_string(&f).map_err(From::from))
        .and_then(|s| fs::write(&outfile, s).map_err(From::from))
        .map(|_| info!("Wrote '{}'", outfile.display()))
}
