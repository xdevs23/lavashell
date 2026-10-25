// SPDX-License-Identifier: GPL-3.0-only
use crate::{
    Error,
    copy::{CopyKey, copy},
};
use lavashell::prelude::*;
use lavashell::renderer::encoded_image;
use std::{
    path::{Path, PathBuf},
    sync::Arc,
};
use tempfile::TempDir;
use url::Url;

pub const PNG: &[u8] = include_bytes!("../assets/sample.png");
pub const PDF: &[u8] = include_bytes!("../assets/sample.pdf");

pub const FORMATS: [&str; 5] = [
    "image/png",
    "application/pdf",
    "text/uri-list",
    "text/plain;charset=utf-8",
    "text/plain",
];

#[derive(Clone, Copy)]
pub enum Source {
    Image,
    Document,
    FileUri,
}

/// The directory of the sample files: a temporary directory created by this process and removed when it is
/// dropped, or the directory of the parent process that started this one.
enum Directory {
    Owned(TempDir),
    Shared(PathBuf),
}

impl Directory {
    fn path(&self) -> &Path {
        match self {
            Self::Owned(directory) => directory.path(),
            Self::Shared(directory) => directory,
        }
    }
}

pub struct SampleFiles {
    directory: Directory,
    pub image: Image,
    png_uri: Arc<[u8]>,
    pdf_uri: Arc<[u8]>,
}

impl SampleFiles {
    /// The sample files in `shared`, written by the parent process, or in a new temporary directory into which this
    /// process writes them.
    pub fn new(shared: Option<PathBuf>) -> Result<Self, Error> {
        let directory = match shared {
            Some(directory) => Directory::Shared(directory),
            None => Directory::Owned(own_directory()?),
        };
        let png = directory.path().join("sample.png");
        let pdf = directory.path().join("sample.pdf");
        if let Directory::Owned(_) = directory {
            write(&png, PNG)?;
            write(&pdf, PDF)?;
        }
        Ok(Self {
            image: encoded_image(PNG).map_err(Error::ReadImage)?,
            png_uri: uri_list(&png)?.into(),
            pdf_uri: uri_list(&pdf)?.into(),
            directory,
        })
    }

    pub fn directory(&self) -> &Path {
        self.directory.path()
    }

    pub fn offer(&self, source: Source) -> Result<TransferData, Error> {
        match source {
            Source::Image => TransferData::new([
                Representation::new("image/png", Arc::<[u8]>::from(PNG)).map_err(Error::Offer)?,
                Representation::new("text/uri-list", self.png_uri.clone()).map_err(Error::Offer)?,
            ]),
            Source::Document => TransferData::new([
                Representation::new("application/pdf", Arc::<[u8]>::from(PDF))
                    .map_err(Error::Offer)?,
                Representation::new(
                    "text/plain;charset=utf-8",
                    copy(CopyKey::MultilineSample).as_bytes(),
                )
                .map_err(Error::Offer)?,
                Representation::new("text/uri-list", self.pdf_uri.clone()).map_err(Error::Offer)?,
            ]),
            Source::FileUri => TransferData::single("text/uri-list", self.pdf_uri.clone()),
        }
        .map_err(Error::Offer)
    }
}

fn own_directory() -> Result<TempDir, Error> {
    let cache = xdg::BaseDirectories::with_prefix("lavashell-interactions")
        .get_cache_home()
        .ok_or(Error::NoHome)?;
    std::fs::create_dir_all(&cache).map_err(|source| Error::CreateCache {
        path: cache.clone(),
        source,
    })?;
    tempfile::tempdir_in(&cache).map_err(Error::CreateSamples)
}

fn write(path: &Path, bytes: &[u8]) -> Result<(), Error> {
    std::fs::write(path, bytes).map_err(|source| Error::WriteSample {
        path: path.to_owned(),
        source,
    })
}

fn uri_list(path: &Path) -> Result<Vec<u8>, Error> {
    let absolute = path.canonicalize().map_err(|source| Error::ResolveSample {
        path: path.to_owned(),
        source,
    })?;
    let uri = Url::from_file_path(&absolute).map_err(|()| Error::FileUri(absolute.clone()))?;
    Ok(format!("{uri}\r\n").into_bytes())
}
