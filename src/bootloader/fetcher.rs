use crate::core::error::{Error, Result};
use std::path::{Path, PathBuf};
use tar::Archive;
use xz2::read::XzDecoder;

/// Git repository fetcher for bootloader files.
#[cfg(feature = "limine")]
pub struct GitFetcher {
    cache_dir: PathBuf,
    verbose: bool,
}

#[cfg(feature = "limine")]
impl GitFetcher {
    /// Create a new git fetcher with the specified cache directory.
    pub fn new(cache_dir: PathBuf, verbose: bool) -> Self {
        Self { cache_dir, verbose }
    }

    /// Fetch a git repository to the cache directory.
    ///
    /// If the repository already exists, it will be used as-is.
    /// If not, it will be cloned from the URL.
    pub fn fetch(&self, url: &str, name: &str, branch: &str) -> Result<PathBuf> {
        let repo_path = self.cache_dir.join(name);

        // If directory exists, assume it's already fetched
        if repo_path.exists() {
            if self.verbose {
                println!("Using cached {} from {}", name, repo_path.display());
            }
            return Ok(repo_path);
        }

        if self.verbose {
            println!("Fetching {} from {}...", name, url);
        }
        std::fs::create_dir_all(&self.cache_dir)?;

        // Clone the repository
        let mut builder = git2::build::RepoBuilder::new();
        builder.branch(branch);

        builder
            .clone(url, &repo_path)
            .map_err(|e| Error::bootloader(format!("failed to clone {}: {}", url, e)))?;

        if self.verbose {
            println!("Fetched {} successfully", name);
        }
        Ok(repo_path)
    }

    /// Fetch a specific commit or tag from a repository.
    pub fn fetch_ref(&self, url: &str, name: &str, git_ref: &str) -> Result<PathBuf> {
        let repo_path = self.cache_dir.join(format!("{}-{}", name, git_ref));

        // If directory exists, assume it's already fetched
        if repo_path.exists() {
            if self.verbose {
                println!("Using cached {} ({}) from {}", name, git_ref, repo_path.display());
            }
            return Ok(repo_path);
        }

        if self.verbose {
            println!("Fetching {} ({}) from {}...", name, git_ref, url);
        }
        std::fs::create_dir_all(&self.cache_dir)?;

        // Clone the repository
        let repo = git2::Repository::clone(url, &repo_path)
            .map_err(|e| Error::bootloader(format!("failed to clone {}: {}", url, e)))?;

        // Checkout the specific ref
        let (object, reference) = repo.revparse_ext(git_ref)
            .map_err(|e| Error::bootloader(format!("failed to find ref {}: {}", git_ref, e)))?;

        repo.checkout_tree(&object, None)
            .map_err(|e| Error::bootloader(format!("failed to checkout {}: {}", git_ref, e)))?;

        match reference {
            Some(gref) => repo.set_head(gref.name().unwrap()),
            None => repo.set_head_detached(object.id()),
        }
        .map_err(|e| Error::bootloader(format!("failed to set HEAD: {}", e)))?;

        if self.verbose {
            println!("Fetched {} ({}) successfully", name, git_ref);
        }
        Ok(repo_path)
    }

    /// Copy files from the fetched repository to a destination.
    pub fn copy_files(&self, repo_path: &Path, files: &[&str], dest_dir: &Path) -> Result<Vec<PathBuf>> {
        let mut copied = Vec::new();

        for file in files {
            let src = repo_path.join(file);
            if !src.exists() {
                return Err(Error::bootloader(format!(
                    "required file not found in repository: {}",
                    file
                )));
            }

            let dest = dest_dir.join(
                Path::new(file)
                    .file_name()
                    .ok_or_else(|| Error::bootloader("invalid file path"))?,
            );

            std::fs::create_dir_all(dest_dir)?;
            std::fs::copy(&src, &dest)?;
            copied.push(dest);
        }

        Ok(copied)
    }
}

/// .tar.xz release fetcher for bootloader files.
#[cfg(feature = "limine")]
pub struct TarXzFetcher {
    cache_dir: PathBuf,
    verbose: bool,
}

#[cfg(feature = "limine")]
impl TarXzFetcher {
    /// Create a new .tar.xz fetcher with the specified cache directory.
    pub fn new(cache_dir: PathBuf, verbose: bool) -> Self {
        Self { cache_dir, verbose }
    }

    /// Create an url for limine release.
    fn create_url(version: &str) -> String {
        format!("https://github.com/Limine-Bootloader/Limine/releases/download/{}/limine-binary.tar.xz", version)
    }

    fn unpack_into(&self, tar_file: &[u8], dir_path: &Path) -> Result<PathBuf> {
        let decompressed = XzDecoder::new(tar_file);
        let mut archive = Archive::new(decompressed);
        archive.unpack(dir_path)?;
        Ok(dir_path.to_path_buf().join("limine-binary"))
    }

    /// Fetch a release to the cache directory.
    ///
    /// If the directory already exists, it will be used as-is.
    /// If not, release will be downloaded and unpacked.
    pub fn fetch(&self, name: &str, version: &str) -> Result<PathBuf> {
        let dir_path = self.cache_dir.join(format!("{}-{}", name, version));
        let binary_path = dir_path.join("limine-binary");

        // If directory exists, assume it's already fetched
        if binary_path.exists() {
            if self.verbose {
                println!("Using cached {} from {}", name, binary_path.display());
            }
            return Ok(binary_path);
        }

        let url = TarXzFetcher::create_url(version);

        if self.verbose {
            println!("Fetching {} from {}...", name, url.as_str());
        }
        std::fs::create_dir_all(&self.cache_dir)?;

        // // Clone the repository
        // let mut builder = git2::build::RepoBuilder::new();
        // builder.branch(branch);
        //
        // builder
        //     .clone(url, &dir_path)
        //     .map_err(|e| Error::bootloader(format!("failed to clone {}: {}", url, e)))?;
        //
        // if self.verbose {
        //     println!("Fetched {} successfully", name);
        // }
        // Fetch the .tar.xz
        let fetched = reqwest::blocking::get(url.as_str())
            .map_err(
                |e| Error::bootloader(format!("failed to fetch {}: {}", url.as_str(), e))
            )?;
        let status = fetched.status();
        if status != reqwest::StatusCode::OK {
            return Err(Error::bootloader(format!("failed to fetch {}: {}", url.as_str(), status)));
        }

        let data = fetched.bytes().map_err(
            |e| Error::bootloader(format!("failed to fetch {}: {}", url.as_str(), e))
        )?;

        self.unpack_into(&data, &dir_path)
    }
}

// Stub implementation when limine feature is disabled
#[cfg(not(feature = "limine"))]
pub struct GitFetcher;

#[cfg(not(feature = "limine"))]
impl GitFetcher {
    pub fn new(_cache_dir: PathBuf, _verbose: bool) -> Self {
        Self
    }

    pub fn fetch(&self, _url: &str, _name: &str, _branch: &str) -> Result<PathBuf> {
        Err(Error::feature_not_enabled("limine"))
    }

    pub fn fetch_ref(&self, _url: &str, _name: &str, _git_ref: &str) -> Result<PathBuf> {
        Err(Error::feature_not_enabled("limine"))
    }

    pub fn copy_files(&self, _repo_path: &Path, _files: &[&str], _dest_dir: &Path) -> Result<Vec<PathBuf>> {
        Err(Error::feature_not_enabled("limine"))
    }
}

#[cfg(not(feature = "limine"))]
pub struct TarXzFetcher;

#[cfg(not(feature = "limine"))]
impl TarXzFetcher {
    pub fn new(_: PathBuf, _: bool) -> Self {
        Self
    }

    pub fn fetch(&self, _: &str, _: &str) -> Result<PathBuf> {
        Err(Error::feature_not_enabled("limine"))
    }
}