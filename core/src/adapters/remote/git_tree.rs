//! Git object names computed without git: what lets a host that has no
//! repository check that a set of files is the tree of a commit.
//!
//! A commit's name is the SHA-1 of its object, the object names its tree,
//! and a tree's name is the SHA-1 of its entries' modes, names, and object
//! names. So files that hash to the tree a commit object names, under a
//! commit object that hashes to the commit's name, are that commit's files.

use std::collections::BTreeMap;
use std::io::Read;
use std::path::Path;

use sha1::{Digest, Sha1};

pub type ObjectId = [u8; 20];

/// How a tree lists a file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileMode {
    Regular,
    Executable,
    Symlink,
}

impl FileMode {
    const fn bytes(self) -> &'static [u8] {
        match self {
            Self::Regular => b"100644",
            Self::Executable => b"100755",
            Self::Symlink => b"120000",
        }
    }
}

fn object_id(kind: &str, length: u64, content: impl FnOnce(&mut Sha1)) -> ObjectId {
    let mut hasher = Sha1::new();
    hasher.update(format!("{kind} {length}\0").as_bytes());
    content(&mut hasher);
    hasher.finalize().into()
}

/// The name of a blob with these bytes.
#[must_use]
pub fn blob_id(bytes: &[u8]) -> ObjectId {
    object_id("blob", bytes.len() as u64, |hasher| hasher.update(bytes))
}

/// The name of the blob a file's bytes are, read in pieces.
pub fn file_blob_id(path: &Path) -> std::io::Result<ObjectId> {
    let mut file = std::fs::File::open(path)?;
    let length = file.metadata()?.len();
    let mut hasher = Sha1::new();
    hasher.update(format!("blob {length}\0").as_bytes());
    let mut buffer = vec![0_u8; 64 * 1024];
    let mut read = 0_u64;
    loop {
        let count = file.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        read = read.saturating_add(count as u64);
        hasher.update(&buffer[..count]);
    }
    if read != length {
        return Err(std::io::Error::other("the file changed while it was read"));
    }
    Ok(hasher.finalize().into())
}

/// The commit a commit object is, and the tree it names. `None` when the
/// bytes are not a commit object.
#[must_use]
pub fn commit_tree(object: &[u8]) -> Option<(String, String)> {
    let named = object
        .strip_prefix(b"tree ")
        .and_then(|rest| rest.get(..40))?;
    let Ok(tree) = std::str::from_utf8(named) else {
        return None;
    };
    if !crate::domain::remote::is_commit_sha(tree) {
        return None;
    }
    if object.get(45) != Some(&b'\n') {
        return None;
    }
    let commit = object_id("commit", object.len() as u64, |hasher| {
        hasher.update(object);
    });
    Some((hex(&commit), tree.to_owned()))
}

#[must_use]
pub fn hex(id: &ObjectId) -> String {
    id.iter().map(|byte| format!("{byte:02x}")).collect()
}

enum Node {
    File(FileMode, ObjectId),
    Directory(BTreeMap<Vec<u8>, Node>),
}

/// The name of the tree that holds exactly `files`: each a path relative
/// to the tree's root with `/` between its parts, its mode, and its blob.
/// Fails for a path that is listed twice or is both a file and a directory.
pub fn tree_id(files: &[(String, FileMode, ObjectId)]) -> Result<ObjectId, String> {
    let mut root = BTreeMap::new();
    for (path, mode, id) in files {
        let mut directory = &mut root;
        let mut parts = path.split('/').peekable();
        while let Some(part) = parts.next() {
            if part.is_empty() || part == "." || part == ".." {
                return Err(format!("'{path}' is not a path inside the tree"));
            }
            let name = part.as_bytes().to_vec();
            if parts.peek().is_none() {
                if directory.insert(name, Node::File(*mode, *id)).is_some() {
                    return Err(format!("'{path}' is listed twice"));
                }
                break;
            }
            let next = directory
                .entry(name)
                .or_insert_with(|| Node::Directory(BTreeMap::new()));
            let Node::Directory(entries) = next else {
                return Err(format!("'{path}' is inside a file"));
            };
            directory = entries;
        }
    }
    Ok(directory_id(&root))
}

fn directory_id(entries: &BTreeMap<Vec<u8>, Node>) -> ObjectId {
    // Git orders a tree's entries by name, with a directory's name taken
    // as if it ended in a slash.
    let mut listed = entries
        .iter()
        .map(|(name, node)| {
            let mut key = name.clone();
            let (mode, id) = match node {
                Node::File(mode, id) => (mode.bytes(), *id),
                Node::Directory(children) => {
                    key.push(b'/');
                    (b"40000".as_slice(), directory_id(children))
                }
            };
            (key, mode, name, id)
        })
        .collect::<Vec<_>>();
    listed.sort_by(|left, right| left.0.cmp(&right.0));
    let mut content = Vec::new();
    for (_, mode, name, id) in listed {
        content.extend_from_slice(mode);
        content.push(b' ');
        content.extend_from_slice(name);
        content.push(0);
        content.extend_from_slice(&id);
    }
    object_id("tree", content.len() as u64, |hasher| {
        hasher.update(&content);
    })
}
