//! Prove callback connection ownership independently of listener-port reuse.
use std::{io::ErrorKind, time::Duration};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
};
use url::Url;

pub struct StalledConnection(TcpStream);

impl StalledConnection {
    pub async fn open(callback: &str) -> Self {
        tokio::time::timeout(Duration::from_secs(3), async {
            let callback = Url::parse(callback).unwrap();
            let mut socket = TcpStream::connect(("127.0.0.1", callback.port().unwrap()))
                .await
                .unwrap();
            socket.set_nodelay(true).unwrap();
            socket
                .write_all(
                    b"GET /done HTTP/1.1\r\nHost: localhost\r\nConnection: keep-alive\r\n\r\n",
                )
                .await
                .unwrap();
            let mut bytes = Vec::new();
            loop {
                let byte = socket.read_u8().await.unwrap();
                bytes.push(byte);
                assert!(
                    bytes.len() < 4096,
                    "callback fixture response exceeded header bound"
                );
                if bytes.ends_with(b"\r\n\r\n") {
                    break;
                }
            }
            let headers = String::from_utf8(bytes).unwrap();
            assert!(
                headers.starts_with("HTTP/1.1 200"),
                "callback connection was not accepted"
            );
            let length: usize = headers
                .lines()
                .find_map(|line| {
                    let (name, value) = line.split_once(':')?;
                    name.eq_ignore_ascii_case("content-length")
                        .then(|| value.trim().parse().unwrap())
                })
                .expect("callback fixture requires a bounded response body");
            assert!(length < 4096);
            let mut body = vec![0; length];
            socket.read_exact(&mut body).await.unwrap();
            // This is the same already-accepted keep-alive connection. The next
            // request deliberately never finishes its headers or reaches a route.
            socket
                .write_all(b"GET /done HTTP/1.1\r\nHost: localhost\r\nX-Stalled: ")
                .await
                .unwrap();
            Self(socket)
        })
        .await
        .expect("callback connection was not accepted within three seconds")
    }

    pub async fn assert_closed(mut self) {
        let mut bytes = [0; 64];
        let result = tokio::time::timeout(Duration::from_secs(3), self.0.read(&mut bytes))
            .await
            .expect("accepted stalled callback connection survived owned cleanup");
        match result {
            Ok(0) => {}
            Err(error)
                if matches!(
                    error.kind(),
                    ErrorKind::ConnectionReset
                        | ErrorKind::ConnectionAborted
                        | ErrorKind::BrokenPipe
                ) => {}
            _ => panic!("callback cleanup did not close the accepted connection"),
        }
    }
}
