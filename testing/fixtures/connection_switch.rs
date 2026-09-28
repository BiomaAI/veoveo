//! Owned TCP fault injection for native database recovery tests.
//! The enclosing harness supplies its deadline and private fixture endpoint.
use tokio::{
    net::{TcpListener, TcpStream},
    sync::{mpsc, oneshot},
    task::{JoinHandle, JoinSet},
};

pub struct ConnectionSwitch {
    pub endpoint: String,
    commands: mpsc::Sender<(bool, oneshot::Sender<()>)>,
    task: JoinHandle<()>,
}

impl ConnectionSwitch {
    pub async fn start(host: String, port: u16) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let endpoint = format!("ws://{}", listener.local_addr().unwrap());
        let (commands, mut receive) = mpsc::channel::<(bool, oneshot::Sender<()>)>(1);
        let task = tokio::spawn(async move {
            let mut enabled = true;
            let mut connections = JoinSet::new();
            loop {
                tokio::select! {
                    command = receive.recv() => {
                        let Some((next, acknowledged)) = command else { break; };
                        enabled = next;
                        if !enabled {
                            connections.abort_all();
                            while connections.join_next().await.is_some() {}
                        }
                        let _ = acknowledged.send(());
                    }
                    accepted = listener.accept() => {
                        let (mut client, _) = accepted.unwrap();
                        if !enabled { continue; }
                        let host = host.clone();
                        connections.spawn(async move {
                            let mut upstream = TcpStream::connect((host.as_str(), port)).await.unwrap();
                            let _ = tokio::io::copy_bidirectional(&mut client, &mut upstream).await;
                        });
                    }
                    _ = connections.join_next(), if !connections.is_empty() => {}
                }
            }
        });
        Self {
            endpoint,
            commands,
            task,
        }
    }

    pub async fn set_enabled(&self, enabled: bool) {
        let (acknowledged, receiver) = oneshot::channel();
        self.commands.send((enabled, acknowledged)).await.unwrap();
        receiver.await.unwrap();
    }
}

impl Drop for ConnectionSwitch {
    fn drop(&mut self) {
        self.task.abort();
    }
}
