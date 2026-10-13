//! Line-delimited JSON intent protocol (sync std::net; async runtime later).
//! Client sends one `Intent` per line; server steps it immediately and
//! replies one JSON object per line:
//! `{"ok":true,"delta":..,"events":..}` or `{"ok":false,"err":".."}`.
//! Message framing this simple keeps the later QUIC/WebSocket swap mechanical.

use crate::Server;
use std::io::{BufRead, Write};

pub fn handle<R: BufRead, W: Write>(
    server: &mut Server,
    reader: R,
    mut writer: W,
) -> std::io::Result<usize> {
    let mut n = 0;
    for line in reader.lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let reply = match serde_json::from_str::<sim::Intent>(&line) {
            Err(e) => serde_json::json!({"ok": false, "err": e.to_string()}),
            Ok(intent) => {
                server.submit(intent);
                let mut out = server.step();
                match out.pop() {
                    Some((delta, events)) => {
                        serde_json::json!({"ok": true, "delta": delta, "events": events})
                    }
                    None => serde_json::json!({"ok": false, "err": "dropped"}),
                }
            }
        };
        writeln!(writer, "{reply}")?;
        writer.flush()?;
        n += 1;
    }
    Ok(n)
}

#[cfg(test)]
mod tests {
    use super::*;
    use sim::{Dir, EntityId, Intent, Pos, Verb};
    use std::io::{BufReader, BufWriter};
    use std::net::{TcpListener, TcpStream};
    use std::time::Duration;

    fn intent_line(d: Dir) -> String {
        serde_json::to_string(&Intent {
            actor: EntityId(1),
            verb: Verb::Move(d),
        })
        .unwrap()
    }

    #[test]
    fn tcp_roundtrip_applies_intents() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let server_thread = std::thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            let mut server = Server::new(5);
            server.world.spawn(
                EntityId(1),
                Pos {
                    city: 1,
                    sx: 0,
                    sy: 0,
                    z: 0,
                    x: 8,
                    y: 8,
                },
                1,
            );
            let read = BufReader::new(stream.try_clone().unwrap());
            let write = BufWriter::new(stream);
            let n = handle(&mut server, read, write).unwrap();
            (n, server.world.snapshot_hash())
        });

        let mut stream = TcpStream::connect(addr).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        writeln!(stream, "{}", intent_line(Dir::E)).unwrap();
        writeln!(stream, "{}", intent_line(Dir::S)).unwrap();
        writeln!(stream, "not json").unwrap();
        let mut reader = BufReader::new(stream.try_clone().unwrap());
        let mut line = String::new();
        reader.read_line(&mut line).unwrap();
        assert!(line.contains("\"ok\":true"), "move accepted: {line}");
        line.clear();
        reader.read_line(&mut line).unwrap();
        assert!(line.contains("\"ok\":true"), "move accepted: {line}");
        line.clear();
        reader.read_line(&mut line).unwrap();
        assert!(line.contains("\"ok\":false"), "bad json rejected: {line}");
        drop(stream);
        drop(reader);
        let (n, _hash) = server_thread.join().unwrap();
        assert_eq!(n, 3);
    }

    #[test]
    fn in_memory_handle_counts_and_skips_blanks() {
        let mut server = Server::new(5);
        server.world.spawn(
            EntityId(1),
            Pos {
                city: 1,
                sx: 0,
                sy: 0,
                z: 0,
                x: 8,
                y: 8,
            },
            1,
        );
        let input = format!("{}\n\nbogus\n", intent_line(Dir::E));
        let mut out = Vec::new();
        let n = handle(&mut server, input.as_bytes(), &mut out).unwrap();
        assert_eq!(n, 2);
        let text = String::from_utf8(out).unwrap();
        assert_eq!(text.lines().count(), 2);
    }
}
