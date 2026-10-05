use super::{
    network::{Download, Hash, Network, hex},
    transfer::{Event, partial_path},
};
use crate::instances::filesystem::Paths;
use sha1::Digest;
use std::{
    io::{Read, Write},
    net::TcpListener,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

fn server(
    responses: Vec<Vec<u8>>,
) -> (String, Arc<Mutex<Vec<String>>>, std::thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    listener.set_nonblocking(true).unwrap();
    let requests = Arc::new(Mutex::new(vec![]));
    let captured = requests.clone();
    let thread = std::thread::spawn(move || {
        for response in responses {
            let start = Instant::now();
            let (mut stream, _) = loop {
                match listener.accept() {
                    Ok(socket) => break socket,
                    Err(e)
                        if e.kind() == std::io::ErrorKind::WouldBlock
                            && start.elapsed() < Duration::from_secs(10) =>
                    {
                        std::thread::sleep(Duration::from_millis(10))
                    }
                    other => panic!("fixture accept: {other:?}"),
                }
            };
            // Accepted sockets may inherit nonblocking mode on Windows.
            stream.set_nonblocking(false).unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            let mut request = vec![];
            while !request.ends_with(b"\r\n\r\n") {
                let mut byte = [0];
                stream.read_exact(&mut byte).unwrap();
                request.push(byte[0]);
            }
            captured
                .lock()
                .unwrap()
                .push(String::from_utf8(request).unwrap().to_lowercase());
            stream.write_all(&response).unwrap();
        }
    });
    (origin, requests, thread)
}
fn response(status: &str, headers: &str, body: &[u8]) -> Vec<u8> {
    let mut bytes = format!("HTTP/1.1 {status}\r\nConnection: close\r\n{headers}\r\n").into_bytes();
    bytes.extend(body);
    bytes
}
fn item(paths: &Paths, origin: &str, body: &[u8]) -> Download {
    Download {
        url: format!("{origin}/artifact"),
        path: paths.root().join("shared/libraries/test.jar"),
        size: body.len() as u64,
        hash: Hash::Sha1(hex(&sha1::Sha1::digest(body))),
    }
}
fn network(origin: &str) -> Network {
    let mut n = Network::new().unwrap();
    n.test_origin = Some(origin.into());
    n
}
#[test]
fn head_reads_declared_artifact_size_not_empty_body_size() {
    let (origin, requests, thread) =
        server(vec![response("200 OK", "Content-Length: 42\r\n", b"")]);
    assert_eq!(
        network(&origin).artifact_size(&format!("{origin}/artifact")),
        Some(42)
    );
    thread.join().unwrap();
    assert!(requests.lock().unwrap()[0].starts_with("head "));
}
#[test]
fn dropped_connection_resumes_and_only_publishes_verified_bytes() {
    let body = b"0123456789abcdef";
    let (origin, requests, thread) = server(vec![
        response("200 OK", "Content-Length: 16\r\n", &body[..6]),
        response(
            "206 Partial Content",
            "Content-Length: 10\r\nContent-Range: bytes 6-15/16\r\n",
            &body[6..],
        ),
    ]);
    let temp = tempfile::tempdir().unwrap();
    let paths = Paths::new(temp.path()).unwrap();
    let item = item(&paths, &origin, body);
    let outcome = network(&origin)
        .download(&paths, &item, &|| false, &|_| {})
        .unwrap();
    thread.join().unwrap();
    assert!(!outcome.cached);
    assert_eq!(std::fs::read(&item.path).unwrap(), body);
    assert!(requests.lock().unwrap()[1].contains("range: bytes=6-"));
    assert!(!partial_path(&paths, &item).unwrap().exists());
    assert!(
        network(&origin)
            .download(&paths, &item, &|| false, &|_| panic!(
                "cache must not download"
            ))
            .unwrap()
            .cached
    );
}
#[test]
fn cancelled_partial_survives_new_client_and_range_ignoring_server_restarts_safely() {
    let body = vec![42; 200_000];
    let (origin, requests, thread) = server(vec![
        response("200 OK", "Content-Length: 200000\r\n", &body),
        response("200 OK", "Content-Length: 200000\r\n", &body),
    ]);
    let temp = tempfile::tempdir().unwrap();
    let paths = Paths::new(temp.path()).unwrap();
    let item = item(&paths, &origin, &body);
    let cancel = AtomicBool::new(false);
    assert!(matches!(
        network(&origin).download(&paths, &item, &|| cancel.load(Ordering::SeqCst), &|_| {
            cancel.store(true, Ordering::SeqCst)
        }),
        Err(crate::error::CoreError::Cancelled)
    ));
    assert!(!item.path.exists());
    assert!(
        partial_path(&paths, &item)
            .unwrap()
            .metadata()
            .unwrap()
            .len()
            > 0
    );
    network(&origin)
        .download(&paths, &item, &|| false, &|_| {})
        .unwrap();
    thread.join().unwrap();
    assert_eq!(std::fs::read(&item.path).unwrap(), body);
    assert!(requests.lock().unwrap()[1].contains("range: bytes="));
}
#[test]
fn corrupt_payload_cannot_replace_existing_file_and_retry_repairs_it() {
    let (origin, _, thread) = server(vec![
        response("200 OK", "Content-Length: 4\r\n", b"evil");
        3
    ]);
    let temp = tempfile::tempdir().unwrap();
    let paths = Paths::new(temp.path()).unwrap();
    let mut item = item(&paths, &origin, b"good");
    std::fs::write(&item.path, b"user modification").unwrap();
    let retries = std::sync::atomic::AtomicU32::new(0);
    assert!(
        network(&origin)
            .download_observed(
                &paths,
                &item,
                &|| false,
                &|_| {},
                &|e| if matches!(e, Event::Retry) {
                    retries.fetch_add(1, Ordering::SeqCst);
                }
            )
            .is_err()
    );
    thread.join().unwrap();
    assert_eq!(retries.load(Ordering::SeqCst), 2);
    assert_eq!(std::fs::read(&item.path).unwrap(), b"user modification");
    let (origin, _, thread) = server(vec![response("200 OK", "Content-Length: 4\r\n", b"good")]);
    item.url = format!("{origin}/artifact");
    assert!(
        network(&origin)
            .download(&paths, &item, &|| false, &|_| {})
            .unwrap()
            .repaired
    );
    thread.join().unwrap();
    assert_eq!(std::fs::read(&item.path).unwrap(), b"good");
}
#[test]
fn wrong_content_range_never_appends_or_publishes() {
    let (origin, _, thread) = server(vec![
        response(
            "206 Partial Content",
            "Content-Length: 2\r\nContent-Range: bytes 0-1/4\r\n",
            b"cd"
        );
        3
    ]);
    let temp = tempfile::tempdir().unwrap();
    let paths = Paths::new(temp.path()).unwrap();
    let item = item(&paths, &origin, b"abcd");
    let part = partial_path(&paths, &item).unwrap();
    paths.mkdir(part.parent().unwrap()).unwrap();
    std::fs::write(&part, b"ab").unwrap();
    assert!(
        network(&origin)
            .download(&paths, &item, &|| false, &|_| {})
            .is_err()
    );
    thread.join().unwrap();
    assert!(!item.path.exists());
    assert_eq!(std::fs::read(part).unwrap(), b"ab");
}
