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
    ops::Deref,
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

// Loaded via the custom `.audio` AssetLoader in the `av` crate: kira's
// symphonia pipeline already sniffs the container format, so we strip the real
// extension on the wire and pick it back up by MIME on the client.
impl IpfsAsset for bevy_kira_audio::AudioSource {
    fn ext() -> &'static str {
        "audio"
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
    // A content hash loaded in the context of a scene. Routes through the
    // scene's registered modifier (e.g. a portable's local content server)
    // rather than falling back to the realm content URL.
    SceneContent {
        scene_hash: String,
        content_hash: String,
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
    IndexDb {
        file_path: String,
    },
}

impl IpfsType {
    pub fn new_content_file(content_hash: String, file_path: String) -> Self {
        Self::ContentFile {
            content_hash,
            file_path: content_file_path(&file_path).into_owned(),
        }
    }

    fn base_url_extension(&self) -> &str {
        match self {
            IpfsType::ContentFile { .. }
            | IpfsType::Entity { .. }
            | IpfsType::SceneContent { .. } => "/contents/",
            IpfsType::UrlCached { .. } => "",
            IpfsType::UrlUncached { .. } => "",
            IpfsType::IndexDb { .. } => "",
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
                    // On an authoritative server a content-map miss must NOT fall through to
                    // fetching the file_path as a raw URL: that turns any asset load of a
                    // missing entry into a scene-controlled outbound request (SSRF). Clients
                    // keep the URL fallthrough.
                    // TODO: check scene.json for allowed domains (include these in context like baseUrls)
                    if common::structs::server_mode() {
                        None
                    } else {
                        url::Url::try_from(file_path.as_str())
                            .is_ok()
                            .then_some(file_path.to_owned())
                    }
                })
                .ok_or_else(|| {
                    anyhow::anyhow!("file not found in content map: {file_path:?} in {scene_hash}")
                }),
            IpfsType::Entity { hash, .. } => Ok(format!("{base_url}{hash}")),
            IpfsType::SceneContent { content_hash, .. } => Ok(format!("{base_url}{content_hash}")),
            IpfsType::UrlCached { url, .. } | IpfsType::UrlUncached { url, .. } => {
                Ok(format!("{}", urlencoding::decode(url)?))
            }
            IpfsType::IndexDb { .. } => panic!(),
        }
    }

    fn hash<'a>(&'a self, context: &'a IpfsContext) -> Option<Cow<'a, str>> {
        let x: Option<Cow<'a, str>> = match self {
            IpfsType::ContentFile {
                content_hash: scene_hash,
                file_path,
                ..
            } => context
                .entities
                .get(scene_hash)?
                .collection
                .hash(file_path)
                .or_else(|| {
                    // if it's not in the content map we assume it's a url
                    let digest = multihash_codetable::Code::Sha2_256.digest(file_path.as_bytes());
                    Some(Cow::Owned(BASE64_URL_SAFE_NO_PAD.encode(digest.digest())))
                }),
            IpfsType::UrlCached { hash, .. } => Some(Cow::Borrowed(hash)),
            IpfsType::Entity { hash, .. } => Some(Cow::Borrowed(hash)),
            IpfsType::SceneContent { content_hash, .. } => Some(Cow::Borrowed(content_hash)),
            IpfsType::UrlUncached { .. } | IpfsType::IndexDb { .. } => None,
        };
        x
    }

    // the container hash if this is a container request, or the path hash otherwise
    fn context_hash(&self) -> Option<&str> {
        match self {
            IpfsType::ContentFile { content_hash, .. } => Some(content_hash),
            IpfsType::UrlCached { .. }
            | IpfsType::UrlUncached { .. }
            | IpfsType::IndexDb { .. } => None,
            IpfsType::Entity { hash, .. } => Some(hash),
            IpfsType::SceneContent { scene_hash, .. } => Some(scene_hash),
        }
    }

    fn context_free_hash(&self) -> Result<Option<&str>, anyhow::Error> {
        match self {
            IpfsType::ContentFile { .. } => {
                anyhow::bail!("Can't get hash for content files without context")
            }
            IpfsType::SceneContent { .. } => {
                anyhow::bail!("Can't get hash for scene content without context")
            }
            IpfsType::UrlCached { .. }
            | IpfsType::UrlUncached { .. }
            | IpfsType::IndexDb { .. } => Ok(None),
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
            IpfsType::SceneContent {
                scene_hash,
                content_hash,
                ext,
            } => PathBuf::from("$scene_content")
                .join(scene_hash)
                .join(format!("{content_hash}.{ext}")),
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
            IpfsType::IndexDb { file_path } => PathBuf::from("$indexdb").join(file_path),
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
                    file_path: content_file_path(&file_path).into_owned(),
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
            "$scene_content" => {
                let scene_hash = components
                    .next()
                    .ok_or(anyhow::anyhow!(
                        "scene content specifier missing scene hash"
                    ))?
                    .to_owned();
                let hash_ext: &str = components.next().ok_or(anyhow::anyhow!(
                    "scene content specifier missing content hash"
                ))?;
                let (content_hash, ext) = hash_ext.split_once('.').ok_or(anyhow::anyhow!(
                    "scene content specified malformed (no '.')"
                ))?;
                Ok(IpfsType::SceneContent {
                    scene_hash,
                    content_hash: content_hash.to_owned(),
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
            "$indexdb" => Ok(IpfsType::IndexDb {
                file_path: components
                    .map(ToOwned::to_owned)
                    .collect::<Vec<_>>()
                    .join("/"),
            }),
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

    pub fn new_indexdb(path: PathBuf) -> Self {
        Self {
            ipfs_type: IpfsType::IndexDb {
                file_path: path.to_string_lossy().into_owned(),
            },
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

    pub fn to_indexdb(&self) -> Option<String> {
        match &self.ipfs_type {
            IpfsType::IndexDb { file_path } => Some(file_path.clone()),
            _ => None,
        }
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

    pub fn hash<'a>(&'a self, context: &'a IpfsContext) -> Option<String> {
        self.ipfs_type.hash(context).map(|h| h.into_owned())
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

    pub fn get(&self, key: &IpfsKey) -> Option<&str> {
        self.key_values.get(key).map(Deref::deref)
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
pub(crate) fn normalize_path(path: &str) -> String {
    path.replace('\\', "/")
}

/// A content file is always addressed relative to its entity's root: `From<&IpfsType> for PathBuf`
/// assembles the `$ipfs/$content_file/<hash>` prefix with `join`s, which an absolute path (or a
/// windows drive prefix) would discard.
///
/// Both `ContentFile` constructors funnel through here, so the invariant holds by construction.
pub(crate) fn content_file_path(file_path: &str) -> Cow<'_, str> {
    if is_http_url(file_path) {
        return Cow::Borrowed(file_path);
    }

    // bytes to drop from the front: a windows drive prefix (`c:/..`), then any root separator,
    // repeated until neither is left - stripping once would let `/c:/x` or `c:/c:/x` keep a drive.
    // a single-letter prefix can't be a scheme we accept, so it is always a drive
    let cut = |s: &str| {
        let mut cut = 0;
        loop {
            let rest = &s[cut..];
            let after_drive = match rest.as_bytes() {
                [drive, b':', ..] if drive.is_ascii_alphabetic() => 2,
                _ => 0,
            };
            let after_root = rest.len() - rest[after_drive..].trim_start_matches('/').len();
            if after_root == 0 {
                return cut;
            }
            cut += after_root;
        }
    };

    if file_path.contains('\\') {
        let mut normalized = normalize_path(file_path);
        normalized.drain(..cut(&normalized));
        Cow::Owned(normalized)
    } else {
        Cow::Borrowed(&file_path[cut(file_path)..])
    }
}

/// True when `file_path` is an http(s) url. A content-map miss hands the file path to the fetcher
/// verbatim, so a url must survive byte-for-byte rather than be rewritten as a path.
///
/// Deliberately not `Url::parse`, and deliberately only http(s): those are the only schemes the
/// fetcher can use, and the wider syntax admits shapes that are better treated as paths.
fn is_http_url(file_path: &str) -> bool {
    let Some((scheme, _)) = file_path.split_once("://") else {
        return false;
    };
    scheme.eq_ignore_ascii_case("http") || scheme.eq_ignore_ascii_case("https")
}

/// Number of leading segments that address the entity rather than a file within it, i.e.
/// everything up to and including the `<hash>` of `$ipfs/../$content_file/<hash>/..`.
fn content_prefix_len(segments: &[&str]) -> Option<usize> {
    segments
        .iter()
        .position(|segment| *segment == "$content_file")
        .map(|idx| idx + 2)
        .filter(|len| *len <= segments.len())
}

/// Path helpers for resolving references made from *inside* an entity's content, where the path
/// carries the `$ipfs/../$content_file/<hash>` prefix.
pub trait ContentPathExt {
    /// Resolve a relative `uri` against this directory, keeping the result inside the entity's own
    /// content collection.
    fn resolve_content_uri(&self, uri: &str) -> PathBuf;
}

impl ContentPathExt for Path {
    fn resolve_content_uri(&self, uri: &str) -> PathBuf {
        let Some(parent) = self.to_str() else {
            return self.join(uri);
        };
        // split the way [`IpfsPath::new_from_path`] does rather than walking [`Path::components`],
        // which drops the empty segment of a `//`. A content-map miss falls through to fetching the
        // file path as a url, so the entity-relative part may itself be one - and rewriting
        // `https://host/x` to `https:/host/x` is exactly what we are here to avoid.
        let parent = if parent.contains('\\') {
            Cow::Owned(normalize_path(parent))
        } else {
            Cow::Borrowed(parent)
        };
        let segments = parent.split('/').collect::<Vec<_>>();

        let Some(prefix_len) = content_prefix_len(&segments) else {
            // not a content-file path - leave the default behaviour alone
            return self.join(uri);
        };

        // force the uri relative first - an absolute one would otherwise discard the prefix
        let uri = content_file_path(uri);

        let mut resolved: Vec<&str> = Vec::new();
        for segment in segments.iter().copied().chain(uri.split('/')) {
            match segment {
                // `.` is noise, but an empty segment carries the `//` of a url, so it is kept
                "." => (),
                ".." if resolved.len() > prefix_len
                    && resolved.last() != Some(&"..")
                    // `prefix_len >= 2`, so the guard above puts this index in range. an empty
                    // segment two back means the last one is a url authority (`https:`, ``,
                    // `host`) rather than a directory, and `..` may not eat that either - a url
                    // resolves its own overshooting `..` against the host
                    && !resolved[resolved.len() - 2].is_empty() =>
                {
                    resolved.pop();
                }
                segment => resolved.push(segment),
            }
        }
        PathBuf::from(resolved.join("/"))
    }
}

/// Fn-pointer form of [`ContentPathExt::resolve_content_uri`], for
/// [`bevy::gltf::GltfPlugin::with_uri_resolver`].
pub fn resolve_content_uri(parent_path: &Path, uri: &str) -> PathBuf {
    parent_path.resolve_content_uri(uri)
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use super::{
        content_file_path, is_http_url, resolve_content_uri, ContentPathExt, IpfsPath, IpfsType,
    };

    #[test]
    fn content_paths_are_forced_relative() {
        assert_eq!(
            content_file_path("models/tree.glb").as_ref(),
            "models/tree.glb"
        );
        assert_eq!(
            content_file_path("/windows/x.png").as_ref(),
            "windows/x.png"
        );
        assert_eq!(
            content_file_path("//host/share/x.png").as_ref(),
            "host/share/x.png"
        );
        assert_eq!(
            content_file_path(r"\windows\x.png").as_ref(),
            "windows/x.png"
        );
        assert_eq!(
            content_file_path("c:/windows/x.png").as_ref(),
            "windows/x.png"
        );
        assert_eq!(
            content_file_path(r"C:\windows\x.png").as_ref(),
            "windows/x.png"
        );
    }

    #[test]
    fn content_paths_leave_urls_alone() {
        // a content-map miss falls through to fetching the file_path as a url, so it must survive
        for url in [
            "https://example.com/a/b.png",
            "http://example.com/v.mp4",
            "https://example.com/a/b.png?x=1&y=2",
            // the `..` belongs to the url, not to us - collapsing it would eat the host
            "https://example.com/a/../b.png",
            // a `\\` in a query or fragment is not a separator - the url parser keeps it
            r"https://example.com/a?x=a\b",
            r"https://example.com/a#frag\ment",
        ] {
            assert_eq!(content_file_path(url).as_ref(), url);
        }
    }

    #[test]
    fn path_shapes_are_never_mistaken_for_urls() {
        // every shape that lets `join` discard its base must fail the url guard and so still be
        // forced relative. only an http(s) scheme is exempt, which none of these have.
        for src in [
            r"c:\passwords.txt://",
            "/windows/x.png://",
            r"\windows\x.png://",
            r"\\host\share://x",
            r"\\?\c:\windows\x.png://",
            "c:/windows/x.png",
            // `c:` is a windows drive plus a root separator, not a scheme
            "c://windows/x.png",
            // a drive behind a root, or behind another drive, is still a drive
            "/c:/windows/x.png",
            "//c:/windows/x.png",
            r"\c:\windows\x.png",
            "c:/c:/windows/x.png",
            "c:/d:/e:/windows/x.png",
            // only http(s) reaches the fetcher, so every other scheme is handled as a path.
            // the transforms happen to leave these untouched - that is asserted, not assumed.
            "file:///windows/x.png",
            "git+ssh://example.com/x.png",
            "chrome-extension://abc/x.png",
            "soap.beep://example.com/x",
        ] {
            assert!(!is_http_url(src), "{src} must not pass as a url");
            let out = content_file_path(src);
            assert!(
                !out.starts_with('/') && !out.starts_with('\\'),
                "{src} -> {out} is still root-relative"
            );
            assert!(
                !matches!(out.as_bytes(), [d, b':', ..] if d.is_ascii_alphabetic()),
                "{src} -> {out} still carries a drive prefix"
            );
        }
    }

    #[test]
    fn an_absolute_src_cannot_shed_the_ipfs_prefix() {
        let ipfs_path = IpfsPath::new(IpfsType::new_content_file(
            "bafyhash".to_owned(),
            "/windows/textures/bark.png".to_owned(),
        ));
        let as_path = PathBuf::from(&ipfs_path);
        assert_eq!(
            as_path,
            Path::new("$ipfs/$content_file/bafyhash/windows/textures/.bark.png")
        );
        assert!(IpfsPath::new_from_path(&as_path).unwrap().is_some());
    }

    #[test]
    fn gltf_uris_resolve_within_the_entity() {
        let parent = Path::new("$ipfs/$content_file/bafyhash/assets/asset-packs/admin_tools");

        assert_eq!(
            resolve_content_uri(parent, "Image_2.png"),
            Path::new("$ipfs/$content_file/bafyhash/assets/asset-packs/admin_tools/Image_2.png")
        );
        // the case that never resolved: a texture shared from outside the model's own folder
        assert_eq!(
            resolve_content_uri(parent, "../../optimized-textures/Image_2.png"),
            Path::new("$ipfs/$content_file/bafyhash/assets/optimized-textures/Image_2.png")
        );
    }

    #[test]
    fn a_uri_can_climb_to_the_entity_root() {
        // pins the prefix length: the first folder below the entity root must still be poppable,
        // otherwise a one-level-up reference resolves to a path that isn't in the content map
        assert_eq!(
            Path::new("$ipfs/$content_file/bafyhash/assets").resolve_content_uri("../x.png"),
            Path::new("$ipfs/$content_file/bafyhash/x.png")
        );
    }

    #[test]
    fn url_sourced_gltf_uris_keep_their_url() {
        // a content-map miss falls through to fetching the file path as a url, so a gltf can be
        // sourced from one - and its own relative uris must still resolve against it
        let parent = Path::new("$ipfs/$content_file/bafyhash/https://example.com/models");

        assert_eq!(
            resolve_content_uri(parent, "tex.png"),
            Path::new("$ipfs/$content_file/bafyhash/https://example.com/models/tex.png")
        );
        assert_eq!(
            resolve_content_uri(parent, "../shared/tex.png"),
            Path::new("$ipfs/$content_file/bafyhash/https://example.com/shared/tex.png")
        );
        // the host is not a directory: `..` past the url root is left for the url parser to
        // resolve (it collapses against the host) rather than eating `example.com`
        assert_eq!(
            resolve_content_uri(parent, "../../tex.png"),
            Path::new("$ipfs/$content_file/bafyhash/https://example.com/../tex.png")
        );
        // an absolute url uri survives byte-for-byte, `//` included
        assert_eq!(
            resolve_content_uri(Path::new("$ipfs/$content_file/bafyhash/models"), URL),
            Path::new("$ipfs/$content_file/bafyhash/models").join(URL)
        );
        const URL: &str = "https://example.com/a/x.png";
    }

    #[test]
    fn gltf_uris_cannot_leave_the_entity() {
        let parent = Path::new("$ipfs/$content_file/bafyhash/assets/models");

        // `..` cannot pop the `$ipfs/$content_file/<hash>` prefix, so a uri can never address
        // another entity's collection or fall out of the ipfs asset source
        for uri in [
            "../../../otherhash/x.png",
            "../../../../../../otherhash/x.png",
            "/windows/x.png",
            "../../../../",
        ] {
            let resolved = resolve_content_uri(parent, uri);
            assert!(
                resolved.starts_with("$ipfs/$content_file/bafyhash"),
                "`{uri}` resolved out of the entity: {resolved:?}"
            );
        }
    }

    #[test]
    fn collectible_metadata_stays_inside_the_entity() {
        // the wearable/emote loaders resolve metadata strings against the entity root itself
        let parent = Path::new("$ipfs/$content_file/bafyhash");
        assert_eq!(
            parent.resolve_content_uri("thumbnail.png"),
            Path::new("$ipfs/$content_file/bafyhash/thumbnail.png")
        );
        for uri in [
            "/windows/x.png",
            r"\windows\x.png",
            "c:/windows/x.png",
            "../otherhash/x.png",
        ] {
            let resolved = parent.resolve_content_uri(uri);
            assert!(
                resolved.starts_with("$ipfs/$content_file/bafyhash"),
                "`{uri}` resolved out of the entity: {resolved:?}"
            );
        }
    }

    #[test]
    fn non_content_paths_keep_the_default_join() {
        let parent = Path::new("some/local/folder");
        assert_eq!(
            resolve_content_uri(parent, "../tex.png"),
            parent.join("../tex.png")
        );
    }

    #[test]
    fn a_parsed_path_cannot_shed_the_ipfs_prefix() {
        // an empty leading component would otherwise assemble an absolute `file_path`
        let ipfs_path = IpfsPath::new_from_path(Path::new("$ipfs/$content_file/bafyhash//x.png"))
            .unwrap()
            .unwrap();
        assert_eq!(ipfs_path.content_path(), Some("x.png"));
        assert!(PathBuf::from(&ipfs_path).starts_with("$ipfs"));
    }
}
