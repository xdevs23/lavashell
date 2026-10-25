// SPDX-License-Identifier: GPL-3.0-only
use crate::Error;
use ash::vk;
use std::ffi::CStr;

/// Vulkan extensions by name: those that an implementation or a physical device offers, or those that an instance
/// or a device enables.
pub trait ExtensionSet {
    fn contains(&self, name: &CStr) -> bool;

    /// # Errors
    /// Fails with the first of `names` that the set lacks.
    fn require(&self, names: &[&'static CStr]) -> Result<(), Error> {
        match names.iter().find(|name| !self.contains(name)) {
            Some(missing) => Err(Error::MissingExtension(missing)),
            None => Ok(()),
        }
    }
}

/// The extensions that a Vulkan implementation or a physical device offers.
pub struct Extensions(Vec<vk::ExtensionProperties>);

impl Extensions {
    pub(crate) fn new(properties: Vec<vk::ExtensionProperties>) -> Self {
        Self(properties)
    }
}

impl ExtensionSet for Extensions {
    fn contains(&self, name: &CStr) -> bool {
        (self.0.iter()).any(|extension| extension.extension_name_as_c_str() == Ok(name))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn offered(names: &[&CStr]) -> Extensions {
        let properties = names.iter().map(|name| {
            let mut extension = vk::ExtensionProperties::default();
            for (target, byte) in extension.extension_name.iter_mut().zip(name.to_bytes()) {
                *target = std::ffi::c_char::from_ne_bytes([*byte]);
            }
            extension
        });
        Extensions::new(properties.collect())
    }

    #[test]
    fn an_offered_extension_is_found_by_its_whole_name() {
        let extensions = offered(&[ash::khr::surface::NAME]);
        assert!(extensions.contains(ash::khr::surface::NAME));
        assert!(!extensions.contains(c"VK_KHR_surf"));
    }

    #[test]
    fn the_first_missing_extension_is_reported() {
        let extensions = offered(&[ash::khr::surface::NAME]);
        let required = [ash::khr::surface::NAME, ash::khr::wayland_surface::NAME];
        assert!(matches!(
            extensions.require(&required),
            Err(Error::MissingExtension(name)) if name == ash::khr::wayland_surface::NAME
        ));
        assert!(extensions.require(&required[..1]).is_ok());
    }
}
