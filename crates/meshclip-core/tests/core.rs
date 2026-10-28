use meshclip_core::{
    clipboard::{hash, Filter},
    model::*,
    security::{verify, Identity},
    transfer::{self, Receiver},
    Service,
};
use std::path::Path;
fn clip(text: &str, time: u64) -> Clip {
    Clip {
        id: uuid::Uuid::new_v4().to_string(),
        source: "a".repeat(64),
        timestamp: time,
        hash: hash(text.as_bytes()),
        text: text.into(),
    }
}
#[test]
fn clipboard_duplicate_loop_and_clock_order() {
    let cfg = Config {
        enabled: true,
        ..Config::default()
    };
    let mut f = Filter::default();
    let c = clip("hello", now());
    assert!(f.admit(&c, &cfg));
    assert!(!f.admit(&c, &cfg));
    assert!(!f.admit(&clip("hello", now() + 1), &cfg));
    assert!(!f.admit(&clip("stale", 1), &cfg));
    assert!(f.admit(&clip("new", now() + 2), &cfg));
}
#[test]
fn clipboard_filters_size_hash_pause_future() {
    let cfg = Config {
        enabled: true,
        max_text: 32,
        ..Config::default()
    };
    for text in ["password=abc", &"x".repeat(33)] {
        assert!(!Filter::default().admit(&clip(text, now()), &cfg));
    }
    let mut bad = clip("good", now());
    bad.hash = "bad".into();
    assert!(!Filter::default().admit(&bad, &cfg));
    assert!(!Filter::default().admit(&clip("future", now() + 600_000), &cfg));
    assert!(!Filter::default().admit(&clip("good", now()), &Config::default()));
}
#[test]
fn identity_signature_and_tampering() {
    let id = Identity::generate().unwrap();
    verify(&id.public(), b"challenge", &id.sign(b"challenge")).unwrap();
    assert!(verify(&id.public(), b"replay", &id.sign(b"challenge")).is_err());
}
fn offer(data: &[u8], name: &str) -> Offer {
    Offer {
        id: uuid::Uuid::new_v4().to_string(),
        name: name.into(),
        size: data.len() as u64,
        hash: hash(data),
    }
}
fn receiver(root: &Path) -> Receiver {
    Receiver::new(&root.join("out"), root, 32 * 1024 * 1024).unwrap()
}
#[test]
fn transfer_empty_and_large_resume_duplicate_chunks() {
    for data in [vec![], vec![42; CHUNK * 12 + 17]] {
        let dir = tempfile::tempdir().unwrap();
        let r = receiver(dir.path());
        let o = offer(&data, "hello.bin");
        assert_eq!(r.accept(&o, "peer").unwrap(), 0);
        let mut offset = 0;
        for (i, b) in data.chunks(CHUNK).enumerate() {
            assert_eq!(
                r.chunk(&o.id, "peer", offset, b).unwrap(),
                offset + b.len() as u64
            );
            assert!(r.chunk(&o.id, "peer", offset, b).is_err());
            offset += b.len() as u64;
            if i == 2 {
                let restarted = receiver(dir.path());
                assert_eq!(restarted.accept(&o, "peer").unwrap(), offset);
            }
        }
        assert_eq!(r.finish(&o.id, "peer").unwrap(), data.len() as u64);
        assert_eq!(
            std::fs::read(dir.path().join("out/hello.bin")).unwrap(),
            data
        );
    }
}
#[test]
fn transfer_security_corruption_cancel_collision() {
    let dir = tempfile::tempdir().unwrap();
    let r = receiver(dir.path());
    let o = offer(b"abc", "safe.txt");
    r.accept(&o, "a").unwrap();
    assert!(r.chunk(&o.id, "b", 0, b"abc").is_err());
    assert!(r.chunk(&o.id, "a", 1, b"abc").is_err());
    r.chunk(&o.id, "a", 0, b"xyz").unwrap();
    assert!(r.finish(&o.id, "a").is_err());
    r.cancel(&o.id, "a").unwrap();
    assert!(r.finish(&o.id, "a").is_err());
    let o = offer(b"abc", "safe.txt");
    r.accept(&o, "a").unwrap();
    r.chunk(&o.id, "a", 0, b"abc").unwrap();
    std::fs::write(dir.path().join("out/safe.txt"), b"existing").unwrap();
    assert!(r.finish(&o.id, "a").is_err());
    assert_eq!(
        std::fs::read(dir.path().join("out/safe.txt")).unwrap(),
        b"existing"
    );
    for name in [
        "../escape",
        "/absolute",
        "C:\\bad",
        "a/b",
        "..",
        "NUL.txt",
        "file.",
        "file ",
    ] {
        assert!(Receiver::validate(&offer(b"a", name), 100).is_err());
    }
    assert!(Receiver::validate(&offer(&[0; 101], "big"), 100).is_err());
}
#[test]
fn config_persistence_and_private_history() {
    let dir = tempfile::tempdir().unwrap();
    let s = Service::open(dir.path()).unwrap();
    let id = s.identity.id();
    let mut c = s.config.lock().unwrap().clone();
    c.name = "persist".into();
    s.configure(c).unwrap();
    drop(s);
    let s = Service::open(dir.path()).unwrap();
    assert_eq!(s.identity.id(), id);
    assert_eq!(s.config.lock().unwrap().name, "persist");
    assert!(s.snapshot().history.is_empty());
}
#[test]
fn source_hash_is_streamed() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("a");
    std::fs::write(&path, b"abc").unwrap();
    assert_eq!(transfer::digest(&path).unwrap(), hash(b"abc"));
}
#[test]
fn folder_archive_preserves_tree_without_auto_extraction() {
    let dir = tempfile::tempdir().unwrap();
    let folder = dir.path().join("folder");
    std::fs::create_dir_all(folder.join("nested")).unwrap();
    std::fs::write(folder.join("nested/a.txt"), b"hello").unwrap();
    let out = dir.path().join("out");
    std::fs::create_dir(&out).unwrap();
    let tar = transfer::pack_folder(&folder, &out, 1024 * 1024).unwrap();
    let mut archive = tar::Archive::new(std::fs::File::open(tar).unwrap());
    let names: Vec<_> = archive
        .entries()
        .unwrap()
        .map(|e| e.unwrap().path().unwrap().to_string_lossy().to_string())
        .collect();
    assert!(names.iter().any(|s| s == "folder/nested/a.txt"));
    assert!(transfer::pack_folder(&folder, &folder, 1024 * 1024).is_err());
}
#[cfg(unix)]
#[test]
fn folder_rejects_symlinks() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir(dir.path().join("in")).unwrap();
    std::fs::create_dir(dir.path().join("out")).unwrap();
    std::os::unix::fs::symlink("/etc/passwd", dir.path().join("in/link")).unwrap();
    assert!(
        transfer::pack_folder(&dir.path().join("in"), &dir.path().join("out"), 1024 * 1024)
            .is_err()
    );
}
#[test]
fn causal_local_clock_advances_even_for_identical_content() {
    let cfg = Config {
        enabled: true,
        ..Config::default()
    };
    let mut f = Filter::default();
    let time = now();
    assert!(f.admit(&clip("a", time), &cfg));
    let same = clip("a", time + 1);
    assert!(!f.admit(&same, &cfg));
    assert!(f.next_timestamp() > time + 1);
    assert!(f.admit(&clip("b", f.next_timestamp()), &cfg));
}

#[test]
fn windows_credential_blob_fits_and_round_trips() {
    let id = Identity::generate().unwrap();
    let blob = id.credential_blob().unwrap();
    assert!(blob.len() <= 2560);
    let restored = Identity::from_credential_blob(&blob).unwrap();
    assert_eq!(restored.id(), id.id());
    assert_eq!(restored.cert, id.cert);
    assert_eq!(restored.tls_key, id.tls_key);
    verify(&restored.public(), b"restored", &restored.sign(b"restored")).unwrap();
}
