use meshclip_core::{clipboard::hash, model::*, Service};
use std::{sync::Arc, time::Duration};
async fn start() -> (
    tempfile::TempDir,
    Arc<Service>,
    tokio::task::JoinHandle<anyhow::Result<()>>,
) {
    let dir = tempfile::tempdir().unwrap();
    let s = Service::open(dir.path()).unwrap();
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    drop(listener);
    let mut c = s.config.lock().unwrap().clone();
    c.listen = addr.to_string();
    c.enabled = true;
    c.history_limit = 10;
    s.configure(c).unwrap();
    let task = tokio::spawn(s.clone().listen());
    for _ in 0..100 {
        if s.snapshot().listening {
            return (dir, s, task);
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("listener not ready")
}
fn authorize(s: &Service) {
    let mut c = s.config.lock().unwrap().clone();
    for p in c.peers.values_mut() {
        p.clipboard = true;
    }
    s.configure(c).unwrap();
}
#[tokio::test]
async fn real_tls_pairing_auth_clipboard_reconnect() {
    let (_da, a, ta) = start().await;
    let (_db, b, tb) = start().await;
    let ticket = b.ticket();
    let peer = ticket.peer.clone();
    assert!(a.rpc(&peer, Request::Ping, &[]).await.is_err());
    a.pair(ticket.clone()).await.unwrap();
    assert!(a.pair(ticket).await.is_err());
    authorize(&a);
    authorize(&b);
    a.publish("A to B".into()).await.unwrap();
    assert_eq!(b.snapshot().history[0].text, "A to B");
    b.publish("A to B".into()).await.unwrap();
    assert!(a.snapshot().history.len() == 1);
    b.publish("B to A".into()).await.unwrap();
    assert_eq!(a.snapshot().history[0].text, "B to A");
    let c = Clip {
        id: uuid::Uuid::new_v4().to_string(),
        source: a.identity.id(),
        timestamp: now() + 1,
        hash: hash(b"once"),
        text: "once".into(),
    };
    a.rpc(&peer, Request::Clipboard(c.clone()), &[])
        .await
        .unwrap();
    a.rpc(&peer, Request::Clipboard(c), &[]).await.unwrap();
    assert_eq!(
        b.snapshot()
            .history
            .iter()
            .filter(|c| c.text == "once")
            .count(),
        1
    );
    tb.abort();
    tokio::time::sleep(Duration::from_millis(20)).await;
    a.publish("offline latest".into()).await.unwrap();
    assert!(a.snapshot().peers.values().any(|v| v.contains("failed")));
    let tb = tokio::spawn(b.clone().listen());
    tokio::time::sleep(Duration::from_millis(20)).await;
    a.start_monitor();
    for _ in 0..100 {
        if b.snapshot()
            .history
            .iter()
            .any(|c| c.text == "offline latest")
        {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert!(b
        .snapshot()
        .history
        .iter()
        .any(|c| c.text == "offline latest"));
    b.revoke(&a.identity.id()).unwrap();
    assert!(a.rpc(&peer, Request::Ping, &[]).await.is_err());
    ta.abort();
    tb.abort();
}
#[tokio::test]
async fn transfer_approval_resume_restart_sha256() {
    let (_da, a, ta) = start().await;
    let (db, b, tb) = start().await;
    a.pair(b.ticket()).await.unwrap();
    let peer = b.ticket().peer;
    let bytes = vec![13; CHUNK * 2 + 7];
    let o = Offer {
        id: uuid::Uuid::new_v4().to_string(),
        name: "payload.bin".into(),
        size: bytes.len() as u64,
        hash: hash(&bytes),
    };
    assert!(a.rpc(&peer, Request::Offer(o.clone()), &[]).await.is_err());
    assert!(a
        .rpc(
            &peer,
            Request::Chunk {
                id: o.id.clone(),
                offset: 0
            },
            &bytes[..CHUNK]
        )
        .await
        .is_err());
    // A restart must not turn a pending request into an approved transfer.
    tb.abort();
    tokio::time::sleep(Duration::from_millis(20)).await;
    drop(b);
    let b = Service::open(db.path()).unwrap();
    assert_eq!(b.snapshot().tasks[&o.id].status, "pending");
    let tb = tokio::spawn(b.clone().listen());
    tokio::time::sleep(Duration::from_millis(20)).await;
    assert!(a
        .rpc(
            &peer,
            Request::Chunk {
                id: o.id.clone(),
                offset: 0
            },
            &bytes[..CHUNK]
        )
        .await
        .is_err());
    b.decide(&o.id, true).unwrap();
    a.rpc(
        &peer,
        Request::Chunk {
            id: o.id.clone(),
            offset: 0,
        },
        &bytes[..CHUNK],
    )
    .await
    .unwrap();
    tb.abort();
    tokio::time::sleep(Duration::from_millis(20)).await;
    drop(b);
    let b = Service::open(db.path()).unwrap();
    let tb = tokio::spawn(b.clone().listen());
    tokio::time::sleep(Duration::from_millis(20)).await;
    let offset = a
        .rpc(&peer, Request::Offer(o.clone()), &[])
        .await
        .unwrap()
        .offset;
    assert_eq!(offset, CHUNK as u64);
    for (i, chunk) in bytes[CHUNK..].chunks(CHUNK).enumerate() {
        a.rpc(
            &peer,
            Request::Chunk {
                id: o.id.clone(),
                offset: ((i + 1) * CHUNK) as u64,
            },
            chunk,
        )
        .await
        .unwrap();
    }
    a.rpc(&peer, Request::Finish { id: o.id.clone() }, &[])
        .await
        .unwrap();
    a.rpc(&peer, Request::Finish { id: o.id.clone() }, &[])
        .await
        .unwrap();
    assert_eq!(
        std::fs::read(db.path().join("received/payload.bin")).unwrap(),
        bytes
    );
    assert_eq!(b.snapshot().tasks[&o.id].status, "complete");
    ta.abort();
    tb.abort();
}
#[tokio::test]
async fn wrong_certificate_rejected() {
    let (_da, a, ta) = start().await;
    let (_db, b, tb) = start().await;
    let (_dc, c, tc) = start().await;
    let mut ticket = b.ticket();
    ticket.peer.cert = c.identity.cert.clone();
    assert!(a.pair(ticket).await.is_err());
    ta.abort();
    tb.abort();
    tc.abort();
}
#[tokio::test]
async fn outgoing_task_real_progress_and_retry() {
    let (da, a, ta) = start().await;
    let (db, b, tb) = start().await;
    a.pair(b.ticket()).await.unwrap();
    let path = da.path().join("source.dat");
    std::fs::write(&path, vec![99; CHUNK * 3 + 1]).unwrap();
    let id = a.queue_file(&b.identity.id(), &path).unwrap();
    for _ in 0..100 {
        if b.snapshot().tasks.contains_key(&id) && a.snapshot().tasks[&id].status == "interrupted" {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    b.decide(&id, true).unwrap();
    a.retry(&id).unwrap();
    for _ in 0..200 {
        if a.snapshot().tasks[&id].status == "complete" {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert_eq!(a.snapshot().tasks[&id].status, "complete");
    assert_eq!(
        std::fs::read(db.path().join("received/source.dat")).unwrap(),
        std::fs::read(path).unwrap()
    );
    assert!(!std::fs::read_to_string(da.path().join("tasks.json"))
        .unwrap()
        .contains("\"history\""));
    ta.abort();
    tb.abort();
}
#[tokio::test]
async fn untrusted_auth_bad_version_and_oversized_frame() {
    use meshclip_core::transport;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let (_da, a, ta) = start().await;
    let (_db, b, tb) = start().await;
    let peer = b.ticket().peer;
    for version in [VERSION, VERSION + 1] {
        let mut stream = transport::connect(&peer.address, &peer.cert).await.unwrap();
        let mut nonce = [0; 32];
        stream.read_exact(&mut nonce).await.unwrap();
        let request = Request::Ping;
        let mut signed = nonce.to_vec();
        signed.extend(serde_json::to_vec(&request).unwrap());
        signed.extend(hash(&[]).as_bytes());
        let auth = Auth {
            version,
            id: a.identity.id(),
            public_key: a.identity.public(),
            signature: a.identity.sign(&signed),
            request,
        };
        transport::write(&mut stream, &auth, &[]).await.unwrap();
        let (reply, _): (Reply, Vec<u8>) = transport::read(&mut stream).await.unwrap();
        assert!(!reply.ok);
        assert!(reply.message.contains(if version == VERSION {
            "untrusted"
        } else {
            "version"
        }));
    }
    let mut stream = transport::connect(&peer.address, &peer.cert).await.unwrap();
    let mut nonce = [0; 32];
    stream.read_exact(&mut nonce).await.unwrap();
    stream.write_u32(3 * 1024 * 1024).await.unwrap();
    let mut data = [0];
    assert!(
        tokio::time::timeout(Duration::from_secs(2), stream.read_exact(&mut data))
            .await
            .unwrap()
            .is_err()
    );
    ta.abort();
    tb.abort();
}
#[tokio::test]
async fn cancelled_receive_never_accepts_more_data() {
    let (_da, a, ta) = start().await;
    let (_db, b, tb) = start().await;
    a.pair(b.ticket()).await.unwrap();
    let p = b.ticket().peer;
    let o = Offer {
        id: uuid::Uuid::new_v4().to_string(),
        name: "cancel.bin".into(),
        size: 3,
        hash: hash(b"abc"),
    };
    assert!(a.rpc(&p, Request::Offer(o.clone()), &[]).await.is_err());
    b.decide(&o.id, true).unwrap();
    b.cancel(&o.id).unwrap();
    assert!(a
        .rpc(
            &p,
            Request::Chunk {
                id: o.id.clone(),
                offset: 0
            },
            b"abc"
        )
        .await
        .is_err());
    assert!(a
        .rpc(&p, Request::Finish { id: o.id.clone() }, &[])
        .await
        .is_err());
    assert_eq!(b.snapshot().tasks[&o.id].status, "cancelled");
    ta.abort();
    tb.abort();
}
#[tokio::test]
async fn simultaneous_clipboard_events_choose_total_order() {
    let (_da, a, ta) = start().await;
    let (_db, b, tb) = start().await;
    let (_dc, c, tc) = start().await;
    a.pair(b.ticket()).await.unwrap();
    c.pair(b.ticket()).await.unwrap();
    authorize(&b);
    let p = b.ticket().peer;
    let time = now();
    let ca = Clip {
        id: "1".repeat(32),
        source: a.identity.id(),
        timestamp: time,
        hash: hash(b"first"),
        text: "first".into(),
    };
    let cc = Clip {
        id: "2".repeat(32),
        source: c.identity.id(),
        timestamp: time,
        hash: hash(b"winner"),
        text: "winner".into(),
    };
    let (ra, rc) = tokio::join!(
        a.rpc(&p, Request::Clipboard(ca), &[]),
        c.rpc(&p, Request::Clipboard(cc), &[])
    );
    ra.unwrap();
    rc.unwrap();
    assert_eq!(b.snapshot().history[0].text, "winner");
    ta.abort();
    tb.abort();
    tc.abort();
}
