use super::*;
#[test]
fn directory_listing_navigation_sort_symlinks_and_invalid_paths() {
    use std::os::unix::{ffi::OsStringExt, fs::symlink};
    let mut env = crate::tests::EnvGuard::new();
    let dir = crate::tests::TempDir::new("gamepad-files");
    env.set("HOME", dir.path());
    dir.write("B.json", b"{}");
    dir.write("a.json", b"{}");
    dir.write("folder/file", b"");
    symlink(dir.join("folder"), dir.join("link")).unwrap();
    symlink(dir.join("missing"), dir.join("broken")).unwrap();
    std::fs::write(dir.join(std::ffi::OsString::from_vec(vec![255])), b"").unwrap();
    for path in [
        None,
        Some("".into()),
        Some("~".into()),
        Some(dir.path().display().to_string()),
    ] {
        let listing = browse_files(path).unwrap();
        assert_eq!(listing.path, dir.path().display().to_string());
        assert!(listing.parent.is_some());
        assert_eq!(listing.entries[0].name, "folder");
        assert!(listing.entries[1].directory);
        assert_eq!(listing.entries[2].name, "a.json");
        assert!(!listing.home.is_empty());
        assert!(!listing.entries[0].path.is_empty());
    }
    assert!(browse_files(Some("~/folder".into()))
        .unwrap()
        .path
        .ends_with("folder"));
    assert!(browse_files(Some("relative".into())).is_err());
    assert!(browse_files(Some(dir.join("missing").display().to_string())).is_err());
    assert!(browse_files(Some(dir.join("a.json").display().to_string())).is_err());
    env.remove("HOME");
    assert!(browse_files(None).is_err());
}
