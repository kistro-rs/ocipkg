//! Pull and Push images to OCI registry based on [OCI distribution specification](https://github.com/opencontainers/distribution-spec)

mod auth;
mod client;
mod name;
mod reference;

pub use auth::*;
pub use client::Client;
pub use name::Name;
pub use reference::Reference;

use crate::{error::*, Digest, ImageName};

use oci_spec::image::*;
use std::{fs, io::Read, path::Path, path::PathBuf};

/// Push image to registry
pub fn push_image(path: &Path) -> Result<()> {
    if !path.is_file() {
        return Err(Error::NotAFile(path.to_owned()));
    }
    let mut f = fs::File::open(path)?;
    let mut ar = crate::image::Archive::new(&mut f);
    for (image_name, manifest) in ar.get_manifests()? {
        log::info!("Push image: {}", image_name);
        let mut client = Client::new(image_name.registry_url()?, image_name.name)?;
        for layer in manifest.layers() {
            let digest = Digest::new(layer.digest())?;
            let mut entry = ar.get_blob(&digest)?;
            let mut buf = Vec::new();
            entry.read_to_end(&mut buf)?;
            client.push_blob(&buf)?;
        }
        let digest = Digest::new(manifest.config().digest())?;
        let mut entry = ar.get_blob(&digest)?;
        let mut buf = Vec::new();
        entry.read_to_end(&mut buf)?;
        client.push_blob(&buf)?;
        client.push_manifest(&image_name.reference, &manifest)?;
    }
    Ok(())
}

/// Get image from registry and save it into local storage
pub fn unpack_image(image_name: &ImageName, dest: &PathBuf) -> Result<()> {
    let ImageName {
        name, reference, ..
    } = image_name;
    let mut client = Client::new(image_name.registry_url()?, name.clone())?;

    log::info!("Get {} into {}", image_name, dest.display());
    let manifest = client.get_manifest(reference)?;
    let layers = manifest.layers();

    for layer in layers {
        let blob = client.get_blob(&Digest::new(layer.digest())?)?;
        log::debug!("[{}] layer {:?}", image_name, blob);
        match layer.media_type() {
            MediaType::ImageLayerGzip => {}
            MediaType::Other(ty) => {
                // application/vnd.docker.image.rootfs.diff.tar.gzip case
                if !ty.ends_with("tar.gzip") {
                    continue;
                }
            }
            _ => continue,
        }
        let buf = flate2::read::GzDecoder::new(blob.as_slice());
        tar::Archive::new(buf).unpack(dest)?;
    }
    if layers.len() == 0 {
        return Err(Error::MissingLayer);
    }
    Ok(())
}

/// Get image from registry and save it into local storage
pub fn get_image(image_name: &ImageName) -> Result<()> {
    let dest = crate::local::image_dir(image_name)?;
    return unpack_image(image_name, &dest);
}
