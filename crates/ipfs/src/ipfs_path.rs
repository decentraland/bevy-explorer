// bevy uses `Path`s to uniquely identify assets, and uses the file extension to determine the asset type and asset loader.
// decentraland ipfs locations don't cleanly map to paths with asset-type extensions so we use the isomorphic wrapper type IpfsPath
// which can be encoded as a `Path` with a suitable extension.
// the wrapper can be converted to an http address (given a context containing a default content endpoint and content mappings), and to/from a path.
//
// in path format, the IpfsPath will consist of:
//
// - the initial marker folder (Component) `$ipfs`
// - zero or more pairs of folders (`Component::Normal`), which are key/value property pairs. The key is prefixed with `&`.
// - a single folder designating the type, prefixed with `$`
// the remainder is determined by the type:
// - `$content_file` -> resolves to a `contents/{hash}` http address
//   - a single folder with the parent entity hash
//   - the remainder of the path is interpreted as the "file" within the entity's `content` collection, including the extension
//   - a leading `.` is added to the terminal filename of the path, to aid in mapping extensions to asset loaders
// - `$entity` -> resolves to a `contents/{hash}` http address
//   - a single filename component, made up of the entity hash and type, e.g. `b64-deadbeef.scene`
// - `$url` -> a raw url
//   - a single filename component, made up of the urlencoded url, and type, e.g. `b64-deadbeef.scene`
//
// key value pairs:
// - `&baseUrl`
//   - a urlencoded endpoint where the resulting entity will be sourced. (this replaces the server address as well as `/contents/` or '/entities/`)

// helper to get a url-encoded path
macro_rules! urlpath {
    ($value: expr) => {
        Path::new(urlencoding::encode($value).as_ref())
    };
}

use std::{
    borrow::Cow,
    collections::BTreeMap,
    ffi::OsStr,
    iter::Peekable,
    path::{Path, PathBuf},
    str::FromStr,
};

use base64::{prelude::BASE64_URL_SAFE_NO_PAD, Engine};
use multihash_codetable::MultihashDigest;

use bevy::log::error;
use urn::Urn;

use crate::ServerAbout;

use super::IpfsContext;

pub trait IpfsAsset: bevy::asset::Asset {
    fn ext() -> &'static str;
}

impl IpfsAsset for bevy::gltf::Gltf {
    fn ext() -> &'static str {
        "gltf"
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum IpfsType {
    ContentFile {
        content_hash: String,
        file_path: String,
    },
    Entity {
        hash: String,
        ext: String,
    },
    UrlCached {
        url: String,
        ext: String,
        hash: String,
    },
    UrlUncached {
        url: String,
        ext: String,
    },
}

impl IpfsType {
    pub fn new_content_file(content_hash: String, file_path: String) -> Self {
        Self::ContentFile {
            content_hash,
            file_path: normalize_path(&file_path),
        }
    }

    fn base_url_extension(&self) -> &str {
        match self {
            IpfsType::ContentFile { .. } | IpfsType::Entity { .. } => "/contents/",
            IpfsType::UrlCached { .. } => "",
            IpfsType::UrlUncached { .. } => "",
        }
    }

    fn url_target(&self, context: &IpfsContext, base_url: &str) -> Result<String, anyhow::Error> {
        match self {
            IpfsType::ContentFile {
                content_hash: scene_hash,
                file_path,
                ..
            } => context
                .entities
                .get(scene_hash)
                .ok_or_else(|| anyhow::anyhow!("required collection hash not found: {scene_hash}"))?
                .collection
                .hash(file_path)
                .map(|hash| format!("{base_url}{hash}"))
                .or_else(|| {
                    // try as a url directly
                    // TODO: check scene.json for allowed domains (include these in context like baseUrls)
                    url::Url::try_from(file_path.as_str())
                        .is_ok()
                        .then_some(file_path.to_owned())
                })
                .ok_or_else(|| {
                    anyhow::anyhow!("file not found in content map: {file_path:?} in {scene_hash}")
                }),
            IpfsType::Entity { hash, .. } => Ok(format!("{base_url}{}", hash)),
            IpfsType::UrlCached { url, .. } | IpfsType::UrlUncached { url, .. } => {
                Ok(format!("{}", urlencoding::decode(url)?))
            }
        }
    }

    fn hash<'a>(&'a self, context: &'a IpfsContext) -> Option<&'a str> {
        match self {
            IpfsType::ContentFile {
                content_hash: scene_hash,
                file_path,
                ..
            } => context.entities.get(scene_hash)?.collection.hash(file_path),
            IpfsType::UrlCached { hash, .. } => Some(hash),
            IpfsType::UrlUncached { .. } => None,
            IpfsType::Entity { hash, .. } => Some(hash),
        }
    }

    // the container hash if this is a container request, or the path hash otherwise
    fn context_hash(&self) -> Option<&str> {
        match self {
            IpfsType::ContentFile { content_hash, .. } => Some(content_hash),
            IpfsType::UrlCached { .. } | IpfsType::UrlUncached { .. } => None,
            IpfsType::Entity { hash, .. } => Some(hash),
        }
    }

    fn context_free_hash(&self) -> Result<Option<&str>, anyhow::Error> {
        match self {
            IpfsType::ContentFile { .. } => {
                anyhow::bail!("Can't get hash for content files without context")
            }
            IpfsType::UrlCached { .. } | IpfsType::UrlUncached { .. } => Ok(None),
            IpfsType::Entity { hash, .. } => Ok(Some(hash)),
        }
    }
}

impl From<&IpfsType> for PathBuf {
    fn from(ipfs_type: &IpfsType) -> Self {
        match ipfs_type {
            IpfsType::ContentFile {
                content_hash: scene_hash,
                file_path,
            } => {
                // add leading `.` to the file_path's filename when converting to path format
                let mut file_path = PathBuf::from(file_path);
                let file_name = format!(
                    ".{}",
                    file_path
                        .file_name()
                        .and_then(OsStr::to_str)
                        .unwrap_or_default()
                );
                file_path.pop();
                file_path.push(file_name);

                PathBuf::from("$content_file")
                    .join(scene_hash)
                    .join(file_path)
            }
            IpfsType::Entity { hash, ext } => {
                PathBuf::from("$entity").join(format!("{hash}.{ext}"))
            }
            IpfsType::UrlCached { url, ext, .. } => PathBuf::from("$urlc").join(format!(
                "{}.{}",
                urlencoding::encode(url).into_owned(),
                ext
            )),
            IpfsType::UrlUncached { url, ext } => PathBuf::from("$urlu").join(format!(
                "{}.{}",
                urlencoding::encode(url).into_owned(),
                ext
            )),
        }
    }
}

impl<'a, I> TryFrom<Peekable<I>> for IpfsType
where
    I: Iterator<Item = &'a str> + std::fmt::Debug,
{
    type Error = anyhow::Error;

    fn try_from(mut components: Peekable<I>) -> Result<Self, Self::Error> {
        let ty = &components
            .next()
            .ok_or(anyhow::anyhow!("missing ipfs type"))?;

        match *ty {
            "$content_file" => {
                let content_hash = components
                    .next()
                    .ok_or(anyhow::anyhow!("content file specifier missing scene hash"))?
                    .to_owned();

                let mut file_path = String::default();
                let mut file_component = components
                    .next()
                    .ok_or(anyhow::anyhow!("content file specifier missing file path"))?;
                // pass through folders
                while components.peek().is_some() {
                    file_path.push_str(file_component);
                    file_path.push('/');
                    file_component = components.next().unwrap();
                }
                // remove the leading '.' from the last component (the file name)
                let stripped_file_name = if let Some(stripped) = file_component.strip_prefix('.') {
                    stripped
                } else {
                    file_component
                };
                file_path.push_str(stripped_file_name);
                Ok(IpfsType::ContentFile {
                    content_hash,
                    file_path: file_path.to_lowercase(),
                })
            }
            "$entity" => {
                let hash_ext: &str = components
                    .next()
                    .ok_or(anyhow::anyhow!("entity specifier missing"))?;
                let (hash, ext) = hash_ext
                    .split_once('.')
                    .ok_or(anyhow::anyhow!("entity specified malformed (no '.')"))?;
                Ok(IpfsType::Entity {
                    hash: hash.to_owned(),
                    ext: ext.to_owned(),
                })
            }
            "$urlc" => {
                let url_ext: &str = components
                    .next()
                    .ok_or(anyhow::anyhow!("url specifier missing"))?;
                let (url, ext) = url_ext
                    .rsplit_once('.')
                    .ok_or(anyhow::anyhow!("url specified malformed (no '.')"))?;

                let digest = multihash_codetable::Code::Sha2_256.digest(url.as_bytes());
                let hash = BASE64_URL_SAFE_NO_PAD.encode(digest.digest());

                Ok(IpfsType::UrlCached {
                    url: url.to_owned(),
                    ext: ext.to_owned(),
                    hash,
                })
            }
            "$urlu" => {
                let url_ext: &str = components
                    .next()
                    .ok_or(anyhow::anyhow!("url specifier missing"))?;
                let (url, ext) = url_ext
                    .rsplit_once('.')
                    .ok_or(anyhow::anyhow!("url specified malformed (no '.')"))?;

                Ok(IpfsType::UrlUncached {
                    url: url.to_owned(),
                    ext: ext.to_owned(),
                })
            }
            _ => anyhow::bail!("invalid ipfs type {ty:?}"),
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub enum IpfsKey {
    BaseUrl,
}

impl AsRef<Path> for IpfsKey {
    fn as_ref(&self) -> &Path {
        match self {
            IpfsKey::BaseUrl => Path::new("&baseUrl"),
        }
    }
}

impl TryFrom<&str> for IpfsKey {
    type Error = anyhow::Error;

    fn try_from(key: &str) -> Result<Self, Self::Error> {
        match key {
            "&baseUrl" => Ok(Self::BaseUrl),
            other => anyhow::bail!("unrecognised ipfs key `{other}`"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct IpfsPath {
    key_values: BTreeMap<IpfsKey, String>,
    ipfs_type: IpfsType,
}

impl IpfsPath {
    pub fn new(ipfs_type: IpfsType) -> Self {
        Self {
            ipfs_type,
            key_values: Default::default(),
        }
    }

    pub fn new_from_urn<T: IpfsAsset>(urn: &str) -> Result<Self, anyhow::Error> {
        let urn = Urn::from_str(urn)?;
        anyhow::ensure!(
            urn.nid() == "decentraland",
            "unrecognised nid {}",
            urn.nid()
        );

        let (lhs, rhs) = urn
            .nss()
            .split_once(':')
            .ok_or_else(|| anyhow::anyhow!("invalid nss `{}`", urn.nss()))?;

        let hash = match lhs {
            "entity" => rhs.to_owned(),
            _ => anyhow::bail!("unrecognised nss lhs: `{lhs}`"),
        };

        let key_values = BTreeMap::from_iter(
            urn.q_component()
                .unwrap_or("")
                .split('&')
                .flat_map(|piece| piece.split_once('='))
                .flat_map(|(key, value)| match key {
                    "baseUrl" => Some((IpfsKey::BaseUrl, value.to_owned())),
                    _ => None,
                }),
        );

        Ok(Self {
            ipfs_type: IpfsType::Entity {
                hash,
                ext: T::ext().to_owned(),
            },
            key_values,
        })
    }

    pub fn new_from_path(path: &Path) -> Result<Option<Self>, anyhow::Error> {
        let Some(path_str) = path.as_os_str().to_str() else {
            return Ok(None);
        };

        let normalized_path = path_str.replace('\\', "/");
        let mut components = normalized_path.split('/').peekable();

        if components.peek() != Some(&"$ipfs") {
            // not an ipfs path
            return Ok(None);
        }
        components.next();

        let mut key_values = BTreeMap::default();
        while components
            .peek()
            .map(|c| c.starts_with('&'))
            .unwrap_or_default()
        {
            let key: IpfsKey = components.next().unwrap().try_into()?;
            let value = components
                .next()
                .and_then(|value| urlencoding::decode(value).ok())
                .map(|value| value.into_owned())
                .ok_or(anyhow::anyhow!("missing value for {key:?}"))?;
            key_values.insert(key, value.to_owned());
        }

        let ipfs_type = components.try_into()?;
        Ok(Some(Self {
            key_values,
            ipfs_type,
        }))
    }

    pub fn new_from_url(url: &str, ext: &str) -> Self {
        let digest = multihash_codetable::Code::Sha2_256.digest(url.as_bytes());
        let hash = BASE64_URL_SAFE_NO_PAD.encode(digest.digest());
        Self {
            key_values: Default::default(),
            ipfs_type: {
                IpfsType::UrlCached {
                    url: url.to_owned(),
                    ext: ext.to_owned(),
                    hash,
                }
            },
        }
    }

    pub fn new_from_url_uncached(url: &str, ext: &str) -> Self {
        Self {
            key_values: Default::default(),
            ipfs_type: {
                IpfsType::UrlUncached {
                    url: url.to_owned(),
                    ext: ext.to_owned(),
                }
            },
        }
    }

    pub fn with_keyvalue(mut self, key: IpfsKey, value: String) -> Self {
        self.key_values.insert(key, value);
        self
    }

    pub fn to_url(&self, context: &IpfsContext) -> Result<String, anyhow::Error> {
        let base_url = self
            // check the embedded base url first
            .key_values
            .get(&IpfsKey::BaseUrl)
            .cloned()
            .or_else(|| {
                // if nothing, check the context modifiers for the hash
                self.ipfs_type.context_hash().and_then(|hash| {
                    context
                        .modifiers
                        .get(hash)
                        .and_then(|modifier| modifier.base_url.to_owned())
                })
            })
            .or_else(|| {
                // fall back to the context base url
                context
                    .about
                    .as_ref()
                    .and_then(ServerAbout::content_url)
                    .map(|base_url| format!("{}{}", base_url, self.ipfs_type.base_url_extension()))
            })
            .ok_or_else(|| anyhow::anyhow!("base url not specified in asset path or context"))?;

        // self.ipfs_type.url_target(context, &base_url)

        let url_str = self.ipfs_type.url_target(context, &base_url)?;
        let url = url::Url::parse(&url_str).map_err(|e| {
            error!("failed to parse as url: {self:?}");
            anyhow::anyhow!(e)
        })?;
        Ok(url.to_string())
    }

    pub fn hash(&self, context: &IpfsContext) -> Option<String> {
        self.ipfs_type.hash(context).map(ToOwned::to_owned)
    }

    pub fn context_free_hash(&self) -> Result<Option<String>, anyhow::Error> {
        Ok(self.ipfs_type.context_free_hash()?.map(ToOwned::to_owned))
    }

    pub fn should_cache(&self, hash: &str) -> bool {
        !hash.starts_with("b64-")
        //        true // TODO only if hash is some and is not b64-
    }

    pub fn base_url(&self) -> Option<&str> {
        self.key_values.get(&IpfsKey::BaseUrl).map(String::as_str)
    }

    pub fn filename(&self) -> Option<Cow<'_, str>> {
        if let IpfsType::ContentFile { file_path, .. } = &self.ipfs_type {
            Path::new(file_path).file_name().map(OsStr::to_string_lossy)
        } else {
            None
        }
    }

    pub fn content_path(&self) -> Option<&str> {
        if let IpfsType::ContentFile { file_path, .. } = &self.ipfs_type {
            Some(file_path)
        } else {
            None
        }
    }
}

impl From<&IpfsPath> for PathBuf {
    fn from(ipfs_path: &IpfsPath) -> Self {
        let mut pb = PathBuf::from("$ipfs");
        for (key, value) in ipfs_path.key_values.iter() {
            pb.push(key);
            pb.push(urlpath!(value));
        }

        pb.join(PathBuf::from(&ipfs_path.ipfs_type))
    }
}

// must be a better way to do this
pub fn normalize_path(path: &str) -> String {
    path.replace('\\', "/")
}
