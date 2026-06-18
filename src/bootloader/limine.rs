use super::{Bootloader, BootloaderFiles, ConfigFile};
use crate::config::BootType;
use crate::core::context::Context;
use crate::core::error::{Error, Result};
use std::path::PathBuf;
use crate::bootloader::fetcher::TarXzFetcher;
#[cfg(feature = "limine")]
use super::GitFetcher;

/// Limine bootloader implementation.
///
/// Limine is a modern, feature-rich bootloader that supports both BIOS and UEFI.
/// This implementation fetches Limine binaries from the official repository and
/// configures them based on the user's limine.conf file.
pub struct LimineBootloader {
    repo_url: String,
}

impl LimineBootloader {
    /// Limine repository URL.
    const DEFAULT_REPO_URL: &'static str = "https://github.com/limine-bootloader/limine.git";

    /// Create a new Limine bootloader instance.
    pub fn new() -> Self {
        Self {
            repo_url: Self::DEFAULT_REPO_URL.to_string(),
        }
    }

    /// Create a Limine bootloader with a custom repository URL.
    pub fn with_repo_url(repo_url: String) -> Self {
        Self { repo_url }
    }

    /// Get the Limine version from config.
    fn get_version<'a>(&self, ctx: &'a Context) -> &'a str {
        &ctx.config.bootloader.limine.version
    }

    /// Fetch Limine binaries.
    /// Use git if version is less than v12, else use .tar.xz.
    #[cfg(feature = "limine")]
    fn fetch_limine(&self, ctx: &Context) -> Result<PathBuf> {
        let version = self.get_version(ctx);
        let cache_dir = ctx.cache_dir.join("bootloaders");

        let major = &version[1..].split(".").next().ok_or(Error::Config(format!(
            "invalid limine version: {}",
            version
        )))?;

        let major: u64 = major
            .parse()
            .map_err(|_| Error::Config(format!("invalid limine version: {}", version)))?;

        if major < 12 {
            let fetcher = GitFetcher::new(cache_dir, ctx.config.verbose);
            fetcher.fetch_ref(&self.repo_url, "limine", version)
        } else {
            let fetcher = TarXzFetcher::new(cache_dir, ctx.config.verbose);
            fetcher.fetch("limine", version)
        }
    }

    /// Stub when limine feature is disabled.
    #[cfg(not(feature = "limine"))]
    fn fetch_limine(&self, _ctx: &Context) -> Result<PathBuf> {
        Err(Error::feature_not_enabled("limine"))
    }
}

impl Default for LimineBootloader {
    fn default() -> Self {
        Self::new()
    }
}

impl Bootloader for LimineBootloader {
    fn prepare(&self, ctx: &Context) -> Result<BootloaderFiles> {
        let limine_folder = self.fetch_limine(ctx)?;

        let mut files = BootloaderFiles::new();

        // Prepare BIOS files if needed
        if ctx.config.boot.boot_type.needs_bios() {
            // Copy limine-bios.sys to boot directory
            let limine_bios = limine_folder.join("limine-bios.sys");
            if !limine_bios.exists() {
                return Err(Error::bootloader(
                    "limine-bios.sys not found in Limine folder. \
                     Make sure the release contains limine-binary-ver.tar.xz or you're using a binary release (e.g., v8.x-binary)."
                        .to_string(),
                ));
            }

            files = files.add_system_file(limine_bios, "limine-bios.sys".into());

            // CD-specific BIOS boot binary for ISO images
            let limine_bios_cd = limine_folder.join("limine-bios-cd.bin");
            if !limine_bios_cd.exists() {
                return Err(Error::bootloader(
                    "limine-bios-cd.bin not found in Limine folder. \
                     Make sure the release contains limine-binary-ver.tar.xz or you're using a binary release (e.g., v8.x-binary)."
                        .to_string(),
                ));
            }
            files = files.add_system_file(limine_bios_cd, "limine-bios-cd.bin".into());
        }

        // Prepare UEFI files if needed
        if ctx.config.boot.boot_type.needs_uefi() {
            // Copy BOOTX64.EFI to EFI/BOOT directory
            let bootx64 = limine_folder.join("BOOTX64.EFI");
            if !bootx64.exists() {
                return Err(Error::bootloader(
                    "BOOTX64.EFI not found in Limine folder. \
                     Make sure the release contains limine-binary-ver.tar.xz or you're using a binary release (e.g., v8.x-binary)."
                        .to_string(),
                ));
            }

            files = files.add_uefi_file(bootx64, "efi/boot/bootx64.efi".into());

            // CD-specific UEFI boot binary for ISO images
            let limine_uefi_cd = limine_folder.join("limine-uefi-cd.bin");
            if !limine_uefi_cd.exists() {
                return Err(Error::bootloader(
                    "limine-uefi-cd.bin not found in Limine folder. \
                     Make sure the release contains limine-binary-ver.tar.xz or you're using a binary release (e.g., v8.x-binary)."
                        .to_string(),
                ));
            }
            files = files.add_system_file(limine_uefi_cd, "limine-uefi-cd.bin".into());
        }

        // Copy the kernel executable to the boot directory
        files = files.add_system_file(
            ctx.executable.clone(),
            PathBuf::from("boot").join(
                ctx.executable
                    .file_name()
                    .ok_or_else(|| Error::config("invalid executable path"))?,
            ),
        );

        Ok(files)
    }

    fn config_files(&self, ctx: &Context) -> Result<Vec<ConfigFile>> {
        let mut configs = Vec::new();

        // Check for limine.conf in the workspace or specified path
        let config_path = if let Some(ref path) = ctx.config.bootloader.config_file {
            ctx.workspace_root.join(path)
        } else {
            ctx.workspace_root.join("limine.conf")
        };

        if config_path.exists() {
            configs.push(
                ConfigFile::new(config_path, "limine.conf".into()).with_template_processing(),
            );
        } else {
            // Generate a default limine.conf if none exists
            return Err(Error::config(format!(
                "limine.conf not found at {}. Please create a Limine configuration file.",
                config_path.display()
            )));
        }

        Ok(configs)
    }

    fn boot_type(&self) -> BootType {
        // Limine supports both BIOS and UEFI
        BootType::Hybrid
    }

    fn name(&self) -> &str {
        "Limine"
    }

    fn validate_config(&self, ctx: &Context) -> Result<()> {
        // Check that version is specified
        let version = self.get_version(ctx);
        if version.is_empty() {
            return Err(Error::config(
                "Limine version not specified in configuration",
            ));
        }

        let major = &version[1..].split(".").next().ok_or(Error::config(format!(
            "invalid limine version: {}",
            version
        )))?;
        let major: u64 = major
            .parse()
            .map_err(|_| Error::config(format!("invalid limine version: {}", version)))?;

        // Recommend binary releases
        if major < 12 && !version.contains("binary") {
            eprintln!(
                "Warning: Limine version '{}' may require building from source. \
                 Consider using at least v12.0 or a binary release like 'v8.x-binary' for faster builds.",
                version
            );
        }

        Ok(())
    }
}
