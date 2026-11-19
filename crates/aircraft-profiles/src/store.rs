//! Windows profile storage: bounded reads, optimistic revisions, atomic replacement.
//! Exclusive file sharing releases automatically on process exit, including a crash.
use crate::{Profile, MAX_PROFILES, MAX_PROFILE_BYTES};
use std::{
    fs::{self, File, OpenOptions},
    io::{self, Read, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};
static NONCE: AtomicU64 = AtomicU64::new(0);
pub struct Store {
    root: PathBuf,
}
#[derive(Default)]
pub struct Catalog {
    pub profiles: Vec<Profile>,
    pub issues: Vec<String>,
}
impl Store {
    pub fn open(root: impl AsRef<Path>) -> io::Result<Self> {
        fs::create_dir_all(root.as_ref())?;
        plain(root.as_ref())?;
        Ok(Self {
            root: root.as_ref().canonicalize()?,
        })
    }
    /// Explicit manual aircraft selection, independent of preview samples.
    pub fn active(&self) -> io::Result<Option<Profile>> {
        let _lock=self.lock()?;
        let path=self.root.join("active.selection");
        if !path.exists(){return Ok(None);}
        plain(&path)?;
        let mut value=String::new();File::open(path)?.take(38).read_to_string(&mut value)?;
        let profile=read_profile(&self.path(value.trim())?)?;
        profile.live()?;Ok(Some(profile))
    }
    pub fn set_active(&self, profile:&Profile) -> io::Result<()> {
        let _lock=self.lock()?;
        profile.live()?;
        if read_profile(&self.path(&profile.profile_uuid)?)? != *profile {return Err(io::Error::other("Profile changed; reopen before applying."));}
        atomic_write(&self.root.join("active.selection"),format!("{}\n",profile.profile_uuid).as_bytes())
    }
    pub fn clear_active(&self)->io::Result<()> {
        let _lock=self.lock()?;let p=self.root.join("active.selection");
        if p.exists(){plain(&p)?;fs::remove_file(p)?;}Ok(())
    }
    pub fn root(&self) -> &Path {
        &self.root
    }
    pub fn export(&self, profile: &Profile, path: &Path) -> io::Result<()> {
        let parent = path
            .parent()
            .ok_or_else(|| io::Error::other("Missing export directory"))?
            .canonicalize()?;
        if parent == self.root {
            return Err(io::Error::other(
                "Export to a folder outside the managed profiles directory.",
            ));
        }
        atomic_write(path, &profile.to_json()?)
    }
    fn lock(&self) -> io::Result<File> {
        let path = self.root.join("profiles.lock");
        if path.exists() {
            plain(&path)?;
        }
        #[cfg(windows)]
        {
            use std::os::windows::fs::OpenOptionsExt;
            OpenOptions::new()
                .read(true)
                .write(true)
                .create(true)
                .truncate(false)
                .share_mode(0)
                .open(path)
        }
        #[cfg(not(windows))]
        {
            let _ = path;
            Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "Profile storage locking currently requires Windows.",
            ))
        }
    }
    fn path(&self, id: &str) -> io::Result<PathBuf> {
        let id = instrument_core::device::Uuid::parse(id)
            .ok_or_else(|| io::Error::other("Invalid profile UUID"))?;
        Ok(self.root.join(format!("{id}.json")))
    }
    pub fn list(&self) -> io::Result<Catalog> {
        let _lock = self.lock()?;
        let mut result = Catalog::default();
        let mut count = 0;
        for entry in fs::read_dir(&self.root)? {
            let path = entry?.path();
            if path.extension().is_none_or(|e| e != "json") {
                continue;
            }
            count += 1;
            if count > MAX_PROFILES {
                return Err(io::Error::other(
                    "Profile directory exceeds the 256-profile limit.",
                ));
            }
            let read = read_profile(&path).and_then(|p| {
                if self.path(&p.profile_uuid)? != path {
                    return Err(io::Error::other("Profile filename does not match UUID."));
                }
                Ok(p)
            });
            match read {
                Ok(p) => result.profiles.push(p),
                Err(e) => result.issues.push(format!(
                    "{}: {e}",
                    path.file_name().unwrap().to_string_lossy()
                )),
            }
        }
        result.profiles.sort_by(|a, b| {
            a.name
                .to_lowercase()
                .cmp(&b.name.to_lowercase())
                .then(a.profile_uuid.cmp(&b.profile_uuid))
        });
        Ok(result)
    }
    pub fn save(&self, draft: &Profile, expected: Option<&Profile>) -> io::Result<Profile> {
        draft.validate()?;
        let _lock = self.lock()?;
        let path = self.path(&draft.profile_uuid)?;
        let current = if path.exists() {
            Some(read_profile(&path)?)
        } else {
            None
        };
        // Compare the complete original, including edits made externally without
        // a revision bump. Revision alone is not a sufficient lost-update guard.
        if current.as_ref() != expected {
            return Err(io::Error::other("This profile changed on disk. Reopen it or save a duplicate; your edits have been kept."));
        }
        if current.is_none() {
            let count = fs::read_dir(&self.root)?
                .filter_map(Result::ok)
                .filter(|e| e.path().extension().is_some_and(|e| e == "json"))
                .count();
            if count >= MAX_PROFILES {
                return Err(io::Error::other("Profile limit reached (256)."));
            }
        }
        let mut saved = draft.clone();
        saved.profile_uuid = instrument_core::device::Uuid::parse(&draft.profile_uuid)
            .unwrap()
            .to_string();
        saved.revision = expected
            .map(|p| p.revision)
            .unwrap_or(0)
            .checked_add(1)
            .ok_or_else(|| io::Error::other("Profile revision exhausted."))?;
        atomic_write(&path, &saved.to_json()?)?;
        Ok(saved)
    }
}
pub fn read_profile(path: &Path) -> io::Result<Profile> {
    plain(path)?;
    let mut bytes = Vec::new();
    File::open(path)?
        .take(MAX_PROFILE_BYTES as u64 + 1)
        .read_to_end(&mut bytes)?;
    Profile::from_json(&bytes).map_err(Into::into)
}
fn plain(path: &Path) -> io::Result<()> {
    let m = fs::symlink_metadata(path)?;
    let mut redirected = m.file_type().is_symlink();
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        redirected |= m.file_attributes() & 0x400 != 0;
    }
    if redirected {
        return Err(io::Error::other(
            "Profile paths must not be links or reparse points.",
        ));
    }
    Ok(())
}
fn atomic_write(path: &Path, bytes: &[u8]) -> io::Result<()> {
    if path.exists() {
        plain(path)?;
    }
    let parent = path
        .parent()
        .ok_or_else(|| io::Error::other("Missing profile directory"))?;
    plain(parent)?;
    let filename = path
        .file_name()
        .ok_or_else(|| io::Error::other("Missing filename"))?
        .to_string_lossy();
    let mut chosen = None;
    for _ in 0..32 {
        let n = NONCE.fetch_add(1, Ordering::Relaxed);
        let temp = parent.join(format!(".{filename}.{}.{n}.tmp", std::process::id()));
        match OpenOptions::new().write(true).create_new(true).open(&temp) {
            Ok(f) => {
                chosen = Some((temp, f));
                break;
            }
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e),
        }
    }
    let (temp, mut f) =
        chosen.ok_or_else(|| io::Error::other("Could not allocate profile staging file."))?;
    let written = f.write_all(bytes).and_then(|_| f.sync_all());
    drop(f);
    let result = written.and_then(|_| fs::rename(&temp, path));
    if result.is_err() {
        let _ = fs::remove_file(&temp);
    }
    result
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;
    use crate::tests::fixture;
    fn root() -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "rfi-profiles-{}-{}",
            std::process::id(),
            NONCE.fetch_add(1, Ordering::Relaxed)
        ));
        // Exclusive creation prevents adopting any pre-existing test directory.
        fs::create_dir(&path).unwrap();
        path
    }
    fn cleanup(root: &Path) {
        let resolved = root.canonicalize().unwrap();
        let temp = std::env::temp_dir().canonicalize().unwrap();
        assert_eq!(resolved.parent(), Some(temp.as_path()));
        assert!(resolved
            .file_name()
            .unwrap()
            .to_string_lossy()
            .starts_with(&format!("rfi-profiles-{}-", std::process::id())));
        fs::remove_dir_all(resolved).unwrap();
    }
    #[test]
    fn active_selection_tracks_saved_revisions_and_rejects_stale_or_invalid_profiles() {
        let root=root();let store=Store::open(&root).unwrap();
        assert!(store.active().unwrap().is_none());
        let one=store.save(&fixture(),None).unwrap();store.set_active(&one).unwrap();
        assert_eq!(store.active().unwrap(),Some(one.clone()));
        let mut edit=one.clone();edit.gauges.airspeed.limits=Some(crate::AirspeedLimits {vs0:45.,vs1:54.,vfe:100.,vno:125.,vne:165.});
        edit.gauges.airspeed.limits_reference="User supplied test limits".into();
        let two=store.save(&edit,Some(&one)).unwrap();assert_eq!(store.active().unwrap(),Some(two));
        assert!(store.set_active(&one).is_err());
        assert_eq!(store.list().unwrap().profiles.len(),1);
        store.clear_active().unwrap();assert!(store.active().unwrap().is_none());cleanup(&root);
    }
    #[test]
    fn saves_are_revisioned_and_conflicting_or_invalid_edits_leave_previous_bytes() {
        let root = root();
        let store = Store::open(&root).unwrap();
        let p = fixture();
        let one = store.save(&p, None).unwrap();
        assert_eq!(one.revision, 1);
        let path = store.path(&p.profile_uuid).unwrap();
        let old = fs::read(&path).unwrap();
        assert!(store.save(&p, None).is_err());
        assert_eq!(fs::read(&path).unwrap(), old);
        let mut bad = one.clone();
        bad.gauges.vertical_speed.full_scale_fpm = 0;
        assert!(store.save(&bad, Some(&one)).is_err());
        assert_eq!(fs::read(&path).unwrap(), old);
        let two = store.save(&one, Some(&one)).unwrap();
        assert_eq!(two.revision, 2);
        assert!(store.save(&one, Some(&one)).is_err());
        assert_eq!(store.list().unwrap().profiles, vec![two]);
        cleanup(&root);
    }
    #[test]
    fn manual_file_edits_with_unchanged_revision_are_not_overwritten() {
        let root = root();
        let store = Store::open(&root).unwrap();
        let p = store.save(&fixture(), None).unwrap();
        let path = store.path(&p.profile_uuid).unwrap();
        let mut external = p.clone();
        external.name = "Changed outside editor".into();
        fs::write(&path, external.to_json().unwrap()).unwrap();
        let mut draft = p.clone();
        draft.name = "Unsent editor change".into();
        assert!(store.save(&draft, Some(&p)).is_err());
        assert_eq!(read_profile(&path).unwrap(), external);
        cleanup(&root);
    }
    #[test]
    fn exported_profiles_roundtrip_and_cannot_overwrite_managed_records() {
        let root = root();
        let store = Store::open(&root).unwrap();
        let p = store.save(&fixture(), None).unwrap();
        let path = store.path(&p.profile_uuid).unwrap();
        let previous = fs::read(&path).unwrap();
        let mut draft = p.clone();
        draft.name = "Edited copy".into();
        assert!(store.export(&draft, &path).is_err());
        assert_eq!(fs::read(&path).unwrap(), previous);
        let exports = root.join("exports");
        fs::create_dir(&exports).unwrap();
        let target = exports.join("shared.json");
        store.export(&draft, &target).unwrap();
        assert_eq!(read_profile(&target).unwrap(), draft);
        let mut imported = read_profile(&target).unwrap();
        imported.profile_uuid = instrument_core::device::Uuid::v4([2; 16]).to_string();
        imported.revision = 0;
        store.save(&imported, None).unwrap();
        assert_eq!(store.list().unwrap().profiles.len(), 2);
        assert_eq!(fs::read(&path).unwrap(), previous);
        cleanup(&root);
    }
    #[test]
    fn bounded_imports_isolate_corrupt_profiles_and_file_lock_recovers_on_close() {
        let root = root();
        let store = Store::open(&root).unwrap();
        store.save(&fixture(), None).unwrap();
        fs::write(root.join("broken.json"), b"not JSON").unwrap();
        let list = store.list().unwrap();
        assert_eq!(list.profiles.len(), 1);
        assert_eq!(list.issues.len(), 1);
        let lock = store.lock().unwrap();
        assert!(store.lock().is_err());
        drop(lock);
        assert!(store.lock().is_ok());
        let huge = root.join("large.json");
        fs::write(&huge, vec![b' '; MAX_PROFILE_BYTES + 1]).unwrap();
        assert!(read_profile(&huge).is_err());
        assert!(store.path("../outside").is_err());
        cleanup(&root);
    }
}
