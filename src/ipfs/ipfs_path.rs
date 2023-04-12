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
// - `$pointer` -> resolves to a `/entities/{type}?pointer={p}` http address
//   - urlencoded `p` folder, (e,g, for scenes, `x,y` where x and y are i32s corresponding to the pointer address)
//   - a `type` filename, which is `type.{type}_pointer`, e.g. `type.scene_pointer`
// - `$entity` -> resolves to a `contents/{hash}` http address
//   - a single filename component, made up of the entity hash and type, e.g. `b64-deadbeef.scene`
//
// key value pairs:
// - `&baseUrl`
//   - a urlencoded endpoint where the resulting entity will be sourced. (this replaces the server address as well as `/contents/` or '/entities/`)

// helper to get a url-encoded path
macro_rules! urlpath {
    ($value: expr) => {
        Path::new(From::<&str>::from(&urlencoding::encode($value)))
    };
}

use std::{
    ffi::OsStr,
    iter::Peekable,
    path::{Component, Components, Path, PathBuf},
};

use bevy::utils::HashMap;

use super::IpfsContext;

pub enum PointerType {
    Scene,
}

impl PointerType {
    fn base_url_extension(&self) -> &str {
        match self {
            PointerType::Scene => "/entities/scene?",
        }
    }
}

impl AsRef<Path> for PointerType {
    fn as_ref(&self) -> &Path {
        match self {
            PointerType::Scene => Path::new("type.scene_pointer"),
        }
    }
}

impl<'a> TryFrom<Component<'a>> for PointerType {
    type Error = anyhow::Error;

    fn try_from(value: Component<'a>) -> Result<Self, Self::Error> {
        match value.as_os_str().to_str() {
            Some("type.scene_pointer") => Ok(PointerType::Scene),
            other => anyhow::bail!("invalid pointer type: {:?}", other),
        }
    }
}

pub enum IpfsType {
    ContentFile {
        content_hash: String,
        file_path: String,
    },
    Pointer {
        pointer_type: PointerType,
        address: String,
    },
    Entity {
        hash: String,
        ext: String,
    },
}

impl IpfsType {
    pub fn new_content_file(content_hash: String, file_path: String) -> Self {
        Self::ContentFile {
            content_hash,
            file_path: normalize_path(&file_path.to_lowercase()),
        }
    }

    fn base_url_extension(&self) -> &str {
        match self {
            IpfsType::ContentFile { .. } | IpfsType::Entity { .. } => "/contents/",
            IpfsType::Pointer { pointer_type, .. } => pointer_type.base_url_extension(),
        }
    }

    fn url_target(&self, context: &IpfsContext) -> Result<String, anyhow::Error> {
        match self {
            IpfsType::ContentFile {
                content_hash: scene_hash,
                file_path,
                ..
            } => context
                .collections
                .get(scene_hash)
                .ok_or_else(|| anyhow::anyhow!("required collection hash not found: {scene_hash}"))?
                .hash(file_path)
                .ok_or_else(|| anyhow::anyhow!("file not found in content map: {file_path:?}"))
                .map(ToOwned::to_owned),
            IpfsType::Pointer {
                pointer_type,
                address,
            } => match pointer_type {
                PointerType::Scene => Ok(format!("pointer={}", address)),
            },
            IpfsType::Entity { hash, .. } => Ok(hash.to_owned()),
        }
    }

    fn hash<'a>(&'a self, context: &'a IpfsContext) -> Option<&'a str> {
        match self {
            IpfsType::ContentFile {
                content_hash: scene_hash,
                file_path,
                ..
            } => context.collections.get(scene_hash)?.hash(file_path),
            IpfsType::Pointer { .. } => None,
            IpfsType::Entity { hash, .. } => Some(hash),
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
                let file_name = format!(".{}", file_path.file_name().unwrap().to_str().unwrap());
                file_path.pop();
                file_path.push(file_name);

                PathBuf::from("$content_file")
                    .join(scene_hash)
                    .join(file_path)
            }
            IpfsType::Pointer {
                pointer_type,
                address,
            } => PathBuf::from("$pointer")
                .join(urlpath!(address))
                .join(pointer_type),
            IpfsType::Entity { hash, ext } => {
                PathBuf::from("$entity").join(format!("{hash}.{ext}"))
            }
        }
    }
}

impl<'a> TryFrom<Peekable<Components<'a>>> for IpfsType {
    type Error = anyhow::Error;

    fn try_from(mut components: Peekable<Components>) -> Result<Self, Self::Error> {
        let ty = &components
            .next()
            .ok_or(anyhow::anyhow!("missing ipfs type"))?
            .as_os_str()
            .to_str();

        match ty {
            Some("$content_file") => {
                let content_hash = components
                    .next()
                    .ok_or(anyhow::anyhow!("content file specifier missing scene hash"))?
                    .as_os_str()
                    .to_string_lossy()
                    .into_owned();

                let mut file_path = PathBuf::default();
                let mut file_component = components
                    .next()
                    .ok_or(anyhow::anyhow!("content file specifier missing file path"))?;
                // pass through folders
                while components.peek().is_some() {
                    file_path.push(file_component);
                    file_component = components.next().unwrap();
                }
                // remove the leading '.' from the last component (the file name)
                let file_name = file_component.as_os_str().to_str().unwrap();
                let stripped_file_name = if let Some(stripped) = file_name.strip_prefix('.') {
                    stripped
                } else {
                    file_name
                };
                file_path.push(stripped_file_name);
                Ok(IpfsType::ContentFile {
                    content_hash,
                    file_path: normalize_path(file_path.to_str().unwrap()),
                })
            }
            Some("$pointer") => {
                let address = urlencoding::decode(
                    &components
                        .next()
                        .ok_or(anyhow::anyhow!("pointer specifier missing address"))?
                        .as_os_str()
                        .to_string_lossy(),
                )?
                .into_owned();
                let pointer_type = components
                    .next()
                    .ok_or(anyhow::anyhow!("pointer specifier missing address"))?
                    .try_into()?;
                Ok(IpfsType::Pointer {
                    pointer_type,
                    address,
                })
            }
            Some("$entity") => {
                let hash_ext: &str = &components
                    .next()
                    .ok_or(anyhow::anyhow!("entity specifier missing"))?
                    .as_os_str()
                    .to_string_lossy();
                let (hash, ext) = hash_ext
                    .rsplit_once('.')
                    .ok_or(anyhow::anyhow!("entity specified malformed (no '.')"))?;
                Ok(IpfsType::Entity {
                    hash: hash.to_owned(),
                    ext: ext.to_owned(),
                })
            }
            _ => anyhow::bail!("invalid ipfs type {ty:?}"),
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
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

impl<'a> TryFrom<Component<'a>> for IpfsKey {
    type Error = anyhow::Error;

    fn try_from(value: Component) -> Result<Self, Self::Error> {
        let key: &str = &value.as_os_str().to_string_lossy();
        match key {
            "&baseUrl" => Ok(Self::BaseUrl),
            other => anyhow::bail!("unrecognised ipfs key `{other}`"),
        }
    }
}

pub struct IpfsPath {
    key_values: HashMap<IpfsKey, String>,
    ipfs_type: IpfsType,
}

impl IpfsPath {
    pub fn new(ipfs_type: IpfsType) -> Self {
        Self {
            ipfs_type,
            key_values: Default::default(),
        }
    }

    pub fn from_path(path: &Path) -> Result<Option<Self>, anyhow::Error> {
        let mut components = path.components().peekable();

        if components.peek() != Some(&Component::Normal(OsStr::new("$ipfs"))) {
            // not an ipfs path
            return Ok(None);
        }
        components.next();

        let mut key_values = HashMap::default();
        while components
            .peek()
            .map(|c| c.as_os_str().to_string_lossy().starts_with('&'))
            .unwrap_or_default()
        {
            let key: IpfsKey = components.next().unwrap().try_into()?;
            let value = components
                .next()
                .ok_or(anyhow::anyhow!("missing value for {key:?}"))?;
            key_values.insert(key, value.as_os_str().to_string_lossy().into_owned());
        }

        let ipfs_type = components.try_into()?;
        Ok(Some(Self {
            key_values,
            ipfs_type,
        }))
    }

    pub fn with_keyvalue(mut self, key: IpfsKey, value: String) -> Self {
        self.key_values.insert(key, value);
        self
    }

    pub fn to_url(&self, context: &IpfsContext) -> Result<String, anyhow::Error> {
        let base_url = self
            .key_values
            .get(&IpfsKey::BaseUrl)
            .cloned()
            .unwrap_or(format!(
                "{}{}",
                context.base_url.as_ref().ok_or_else(|| anyhow::anyhow!(
                    "base url not specified in asset path or context"
                ))?,
                self.ipfs_type.base_url_extension()
            ));

        let target = self.ipfs_type.url_target(context)?;

        Ok(format!("{base_url}{target}"))
    }

    pub fn hash(&self, context: &IpfsContext) -> Option<String> {
        self.ipfs_type.hash(context).map(ToOwned::to_owned)
    }

    pub fn should_cache(&self) -> bool {
        true // TODO only if hash is some and is not b64-
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
    path.to_lowercase().replace('\\', "/")
}
