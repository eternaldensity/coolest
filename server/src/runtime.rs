//! Async runtime (design/08): tokio TCP listener, one task per connection,
//! single shared Server behind a mutex. Arrival order = application order,
//! so determinism and replay keep working exactly as in the sync path.

use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::Mutex;

use crate::Server;

pub struct Runtime {
    pub server: Arc<Mutex<Server>>,
}

impl Runtime {
    pub fn new(seed: u64) -> Self {
        Self { server: Arc::new(Mutex::new(Server::new(seed))) }
    }

    /// Serve forever. One task per connection; intents apply in arrival order.
    pub async fn serve(&self, listener: TcpListener) -> std::io::Result<()> {
        loop {
            let (stream, _) = listener.accept().await?;
            let server = self.server.clone();
            tokio::spawn(async move {
                if let Err(e) = handle_conn(server, stream).await {
                    eprintln!("conn error: {e}");
                }
            });
        }
    }
}

async fn handle_conn(server: Arc<Mutex<Server>>, stream: TcpStream) -> std::io::Result<()> {
    let (read, mut write) = stream.into_split();
    let mut lines = BufReader::new(read).lines();
    while let Some(line) = lines.next_line().await? {
        if line.trim().is_empty() {
            continue;
        }
        let reply = match serde_json::from_str::<sim::Intent>(&line) {
            Err(e) => serde_json::json!({"ok": false, "err": e.to_string()}),
            Ok(intent) => {
                let mut s = server.lock().await;
                s.submit(intent);
                let mut out = s.step();
                match out.pop() {
                    Some((delta, events)) => serde_json::json!({"ok": true, "delta": delta, "events": events}),
                    None => serde_json::json!({"ok": false, "err": "dropped"}),
                }
            }
        };
        write.write_all(format!("{reply}\n").as_bytes()).await?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use sim::{Dir, EntityId, Intent, Pos, Verb};

    fn spawn_setup(rt: &Runtime) {
        let mut s = rt.server.try_lock().unwrap();
        s.world.spawn(EntityId(1), Pos { city: 1, sx: 0, sy: 0, z: 0, x: 8, y: 8 }, 1);
        s.world.spawn(EntityId(2), Pos { city: 1, sx: 0, sy: 0, z: 0, x: 4, y: 4 }, 1);
    }

    async fn send(addr: std::net::SocketAddr, lines: &[String]) -> Vec<String> {
        let stream = TcpStream::connect(addr).await.unwrap();
        let (read, mut write) = stream.into_split();
        for l in lines {
            write.write_all(format!("{l}\n").as_bytes()).await.unwrap();
        }
        let mut out = Vec::new();
        let mut lr = BufReader::new(read).lines();
        for _ in 0..lines.len() {
            out.push(lr.next_line().await.unwrap().unwrap());
        }
        out
    }

    fn intent(actor: u64, d: Dir) -> String {
        serde_json::to_string(&Intent { actor: EntityId(actor), verb: Verb::Move(d) }).unwrap()
    }

    #[tokio::test]
    async fn two_clients_share_one_world() {
        let rt = Runtime::new(3);
        spawn_setup(&rt);
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = rt.server.clone();
        let task = tokio::spawn(async move {
            let runtime = Runtime { server };
            runtime.serve(listener).await
        });

        let r1 = send(addr, &[intent(1, Dir::E)]).await;
        assert!(r1[0].contains("\"ok\":true"), "{}", r1[0]);
        let r2 = send(addr, &[intent(2, Dir::S), "garbage".to_string()]).await;
        assert!(r2[0].contains("\"ok\":true"), "{}", r2[0]);
        assert!(r2[1].contains("\"ok\":false"), "{}", r2[1]);
        task.abort();

        let s = rt.server.lock().await;
        assert_eq!(s.log.len(), 2); // garbage never logged
    }
}
