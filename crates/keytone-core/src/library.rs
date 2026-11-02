//! The on-disk pack library: listing, installing bundled packs, duplicating,
//! deleting, importing (folder / zip / Mechvibes) and exporting.

use crate::mechvibes;
use crate::pack::{safe_file_name, PackFile, PACK_FILE};
use crate::sound::decode_file;
use anyhow::{anyhow, bail, Context, Result};
use serde::Serialize;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

pub const EXPORT_EXT: &str = "keytone";

#[derive(Debug, Clone, Serialize)]
pub struct PackSummary {
    pub id: String,
    pub name: String,
    pub author: String,
    pub description: String,
    pub has_cover: bool,
    pub key_overrides: usize,
}

pub struct Library {
    root: PathBuf,
}

impl Library {
    pub fn open(root: impl Into<PathBuf>) -> Result<Library> {
        let root = root.into();
        std::fs::create_dir_all(&root).with_context(|| format!("creating {}", root.display()))?;
        Ok(Library { root })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn dir_of(&self, id: &str) -> Result<PathBuf> {
        Ok(self.root.join(safe_file_name(id)?))
    }

    pub fn list(&self) -> Vec<PackSummary> {
        let mut out: Vec<PackSummary> = std::fs::read_dir(&self.root)
            .into_iter()
            .flatten()
            .flatten()
            .filter(|e| e.path().join(PACK_FILE).is_file())
            .filter_map(|e| {
                let dir = e.path();
                let pf = PackFile::read(&dir).ok()?;
                // The folder name is the source of truth for the id.
                let id = dir.file_name()?.to_str()?.to_string();
                Some(PackSummary {
                    has_cover: pf.cover.as_ref().map(|c| dir.join(c).is_file()).unwrap_or(false),
                    key_overrides: pf.keys.len(),
                    id,
                    name: pf.name,
                    author: pf.author,
                    description: pf.description,
                })
            })
            .collect();
        out.sort_by_key(|a| a.name.to_lowercase());
        out
    }

    pub fn read(&self, id: &str) -> Result<PackFile> {
        let mut pf = PackFile::read(&self.dir_of(id)?)?;
        pf.id = id.to_string();
        Ok(pf)
    }

    pub fn write(&self, id: &str, pf: &PackFile) -> Result<()> {
        let mut pf = pf.clone();
        pf.id = id.to_string();
        pf.prune();
        pf.write(&self.dir_of(id)?)
    }

    /// Copy bundled packs into the library unless a pack with that id
    /// already exists (so user edits are never overwritten) or it is listed
    /// in `skip` (installed before and since deleted by the user).
    /// Returns the ids that were copied.
    pub fn install_bundled(&self, bundled_root: &Path, skip: &[String]) -> Result<Vec<String>> {
        let mut installed = Vec::new();
        for entry in std::fs::read_dir(bundled_root)?.flatten() {
            let src = entry.path();
            if !src.join(PACK_FILE).is_file() {
                continue;
            }
            let Some(name) = src.file_name().and_then(|n| n.to_str()) else { continue };
            let dst = self.root.join(name);
            if dst.exists() || skip.iter().any(|s| s == name) {
                continue;
            }
            copy_dir(&src, &dst)?;
            installed.push(name.to_string());
        }
        Ok(installed)
    }

    pub fn unique_id(&self, base: &str) -> String {
        let base = slugify(base);
        let base = if base.is_empty() { "pack".to_string() } else { base };
        if !self.root.join(&base).exists() {
            return base;
        }
        (2..)
            .map(|i| format!("{base}-{i}"))
            .find(|c| !self.root.join(c).exists())
            .expect("infinite iterator")
    }

    pub fn duplicate(&self, id: &str, new_name: &str) -> Result<String> {
        let src = self.dir_of(id)?;
        let new_id = self.unique_id(new_name);
        let dst = self.root.join(&new_id);
        copy_dir(&src, &dst)?;
        let mut pf = PackFile::read(&dst)?;
        pf.id = new_id.clone();
        pf.name = new_name.to_string();
        pf.write(&dst)?;
        Ok(new_id)
    }

    pub fn create_empty(&self, name: &str) -> Result<String> {
        let id = self.unique_id(name);
        let dir = self.root.join(&id);
        std::fs::create_dir_all(&dir)?;
        PackFile::new(&id, name).write(&dir)?;
        Ok(id)
    }

    pub fn delete(&self, id: &str) -> Result<()> {
        let dir = self.dir_of(id)?;
        if !dir.join(PACK_FILE).is_file() {
            bail!("{id} is not a pack");
        }
        std::fs::remove_dir_all(dir)?;
        Ok(())
    }

    /// Copy an audio file into a pack, validating that it decodes and is
    /// short. Returns the (possibly de-duplicated) file name used.
    pub fn add_sound_file(&self, id: &str, src: &Path) -> Result<String> {
        decode_file(src, crate::pack::MAX_CLIP_SECS)?;
        let dir = self.dir_of(id)?;
        let stem = src.file_stem().and_then(|s| s.to_str()).unwrap_or("sound");
        let ext = src.extension().and_then(|s| s.to_str()).unwrap_or("wav").to_lowercase();
        let name = unique_file(&dir, &slugify(stem), &ext);
        std::fs::copy(src, dir.join(&name))?;
        Ok(name)
    }

    /// Pick an unused file name inside a pack (for recordings).
    pub fn new_file_name(&self, id: &str, stem: &str, ext: &str) -> Result<(String, PathBuf)> {
        let dir = self.dir_of(id)?;
        let name = unique_file(&dir, &slugify(stem), ext);
        let path = dir.join(&name);
        Ok((name, path))
    }

    /// Import a pack from a folder or a `.keytone`/`.zip` archive. Handles
    /// Keytone packs and Mechvibes packs. Returns the new pack id.
    pub fn import(&self, src: &Path) -> Result<String> {
        if src.is_dir() {
            return self.import_dir(src);
        }
        let tmp = self.root.join(format!(".import-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp)?;
        let res = extract_zip(src, &tmp).and_then(|_| {
            let dir = find_pack_root(&tmp).ok_or_else(|| anyhow!("archive contains no pack.json or Mechvibes config.json"))?;
            self.import_dir(&dir)
        });
        let _ = std::fs::remove_dir_all(&tmp);
        res
    }

    fn import_dir(&self, src: &Path) -> Result<String> {
        let src = find_pack_root(src).ok_or_else(|| anyhow!("folder contains no pack.json or Mechvibes config.json"))?;
        if mechvibes::is_mechvibes_pack(&src) {
            let name = read_mv_name(&src).unwrap_or_else(|| "Imported pack".into());
            let id = self.unique_id(&name);
            let dst = self.root.join(&id);
            std::fs::create_dir_all(&dst)?;
            if let Err(e) = mechvibes::convert(&src, &dst, &id) {
                let _ = std::fs::remove_dir_all(&dst);
                return Err(e);
            }
            return Ok(id);
        }
        let pf = PackFile::read(&src)?;
        let id = self.unique_id(&pf.name);
        let dst = self.root.join(&id);
        copy_dir(&src, &dst)?;
        let mut pf = PackFile::read(&dst)?;
        pf.id = id.clone();
        pf.write(&dst)?;
        Ok(id)
    }

    /// Export a pack as a zip archive (`.keytone`).
    pub fn export(&self, id: &str, dest: &Path) -> Result<()> {
        let dir = self.dir_of(id)?;
        let file = std::fs::File::create(dest).with_context(|| format!("creating {}", dest.display()))?;
        let mut zip = zip::ZipWriter::new(file);
        let opts = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
        for entry in std::fs::read_dir(&dir)?.flatten() {
            let p = entry.path();
            let Some(name) = p.file_name().and_then(|n| n.to_str()) else { continue };
            if !p.is_file() || name.starts_with('.') {
                continue;
            }
            zip.start_file(format!("{id}/{name}"), opts)?;
            let mut buf = Vec::new();
            std::fs::File::open(&p)?.read_to_end(&mut buf)?;
            zip.write_all(&buf)?;
        }
        zip.finish()?;
        Ok(())
    }
}

fn read_mv_name(dir: &Path) -> Option<String> {
    let v: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(dir.join("config.json")).ok()?).ok()?;
    v.get("name")?.as_str().map(str::to_string)
}

/// A pack may sit at the root or one folder down (typical for zips).
fn find_pack_root(dir: &Path) -> Option<PathBuf> {
    let is_pack = |d: &Path| d.join(PACK_FILE).is_file() || d.join("config.json").is_file();
    if is_pack(dir) {
        return Some(dir.to_path_buf());
    }
    std::fs::read_dir(dir)
        .ok()?
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_dir() && !p.file_name().map(|n| n.to_string_lossy().starts_with("__MACOSX")).unwrap_or(false))
        .find(|p| is_pack(p))
}

fn extract_zip(src: &Path, dst: &Path) -> Result<()> {
    let file = std::fs::File::open(src).with_context(|| format!("opening {}", src.display()))?;
    let mut zip = zip::ZipArchive::new(file).context("not a valid zip/.keytone archive")?;
    let mut total: u64 = 0;
    for i in 0..zip.len() {
        let mut f = zip.by_index(i)?;
        // `enclosed_name` rejects absolute paths and `..` (zip-slip).
        let Some(rel) = f.enclosed_name() else { continue };
        total += f.size();
        if total > 500 * 1024 * 1024 {
            bail!("archive is too large");
        }
        let out = dst.join(rel);
        if f.is_dir() {
            std::fs::create_dir_all(&out)?;
            continue;
        }
        if let Some(parent) = out.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut w = std::fs::File::create(&out)?;
        std::io::copy(&mut f, &mut w)?;
    }
    Ok(())
}

fn copy_dir(src: &Path, dst: &Path) -> Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)?.flatten() {
        let p = entry.path();
        if p.is_file() {
            std::fs::copy(&p, dst.join(entry.file_name()))?;
        }
    }
    Ok(())
}

fn unique_file(dir: &Path, stem: &str, ext: &str) -> String {
    let stem = if stem.is_empty() { "sound" } else { stem };
    let first = format!("{stem}.{ext}");
    if !dir.join(&first).exists() {
        return first;
    }
    (2..).map(|i| format!("{stem}-{i}.{ext}")).find(|n| !dir.join(n).exists()).unwrap()
}

pub fn slugify(s: &str) -> String {
    let mut out = String::new();
    let mut dash = false;
    for c in s.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
            dash = false;
        } else if !dash && !out.is_empty() {
            out.push('-');
            dash = true;
        }
    }
    while out.ends_with('-') {
        out.pop();
    }
    out.truncate(48);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pack::{LoadedPack, Slot};
    use crate::sound::Sound;

    fn lib() -> (tempfile::TempDir, Library) {
        let d = tempfile::tempdir().unwrap();
        let l = Library::open(d.path().join("packs")).unwrap();
        (d, l)
    }

    fn make_pack(l: &Library, name: &str) -> String {
        let id = l.create_empty(name).unwrap();
        let dir = l.dir_of(&id).unwrap();
        Sound::from_mono(8000, &vec![0.2; 400]).write_wav(&dir.join("k.wav")).unwrap();
        let mut pf = l.read(&id).unwrap();
        pf.defaults = Slot { down: Some(vec!["k.wav".into()]), up: None };
        l.write(&id, &pf).unwrap();
        id
    }

    #[test]
    fn slugs() {
        assert_eq!(slugify("1920s Typewriter!"), "1920s-typewriter");
        assert_eq!(slugify("  --  "), "");
        assert_eq!(slugify("Ñandú Clicks"), "and-clicks");
    }

    #[test]
    fn create_duplicate_delete() {
        let (_d, l) = lib();
        let id = make_pack(&l, "My Pack");
        assert_eq!(id, "my-pack");
        let dup = l.duplicate(&id, "My Pack").unwrap();
        assert_eq!(dup, "my-pack-2");
        assert_eq!(l.list().len(), 2);
        assert!(LoadedPack::load(&l.dir_of(&dup).unwrap()).is_ok());
        l.delete(&dup).unwrap();
        assert_eq!(l.list().len(), 1);
        assert!(l.delete("../etc").is_err());
    }

    #[test]
    fn export_import_roundtrip() {
        let (d, l) = lib();
        let id = make_pack(&l, "Round Trip");
        let zip = d.path().join("rt.keytone");
        l.export(&id, &zip).unwrap();
        let new_id = l.import(&zip).unwrap();
        assert_eq!(new_id, "round-trip-2");
        let p = LoadedPack::load(&l.dir_of(&new_id).unwrap()).unwrap();
        assert_eq!(p.resolve("KeyA", keyhook::Dir::Down).len(), 1);
        // no temp folders left behind
        assert!(std::fs::read_dir(l.root()).unwrap().flatten().all(|e| !e.file_name().to_string_lossy().starts_with('.')));
    }

    #[test]
    fn install_bundled_does_not_overwrite() {
        let (d, l) = lib();
        let bundled = d.path().join("bundled");
        let b = Library::open(&bundled).unwrap();
        let id = make_pack(&b, "Bundled One");
        assert_eq!(l.install_bundled(&bundled, &[]).unwrap(), vec![id.clone()]);
        let mut pf = l.read(&id).unwrap();
        pf.name = "Edited".into();
        l.write(&id, &pf).unwrap();
        assert!(l.install_bundled(&bundled, &[]).unwrap().is_empty());
        assert_eq!(l.read(&id).unwrap().name, "Edited");
        // A deleted bundled pack stays deleted.
        l.delete(&id).unwrap();
        assert!(l.install_bundled(&bundled, std::slice::from_ref(&id)).unwrap().is_empty());
    }

    #[test]
    fn add_sound_validates() {
        let (d, l) = lib();
        let id = make_pack(&l, "S");
        let good = d.path().join("My Click.wav");
        Sound::from_mono(8000, &vec![0.2; 400]).write_wav(&good).unwrap();
        assert_eq!(l.add_sound_file(&id, &good).unwrap(), "my-click.wav");
        assert_eq!(l.add_sound_file(&id, &good).unwrap(), "my-click-2.wav");
        let bad = d.path().join("notaudio.wav");
        std::fs::write(&bad, b"hello").unwrap();
        assert!(l.add_sound_file(&id, &bad).is_err());
    }
}
