use form_data_builder::FormData;
use rustls::{OwnedTrustAnchor, RootCertStore, ClientConfig, ClientConnection, Stream};
use std::{
    ffi::OsStr,
    io::{Cursor, Read, Write},
    net::{Shutdown, TcpStream},
    time::Duration, sync::Arc,
};

pub struct Frame {
    pub bytes: Vec<u8>,
    pub channel: u64,
    tcp_stream: Option<TcpStream>,
    cache_stream: Option<ClientConnection>,
    byte_to_write: Option<u8>,
}

impl Frame {
    pub fn new(bytes: Vec<u8>, channel: u64) -> Frame {
        Frame {
            bytes,
            channel,
            tcp_stream: None,
            cache_stream: None,
            byte_to_write: None,
        }
    }

    pub fn cache_frame(&mut self, message: u64, content: &str, token: &str) {
        let mut root_store = RootCertStore::empty();
        root_store.add_server_trust_anchors(webpki_roots::TLS_SERVER_ROOTS.0.iter().map(|ta| {
            OwnedTrustAnchor::from_subject_spki_name_constraints(
                ta.subject,
                ta.spki,
                ta.name_constraints,
            )
        }));
        let client_config = Arc::new(ClientConfig::builder()
            .with_safe_defaults()
            .with_root_certificates(root_store)
            .with_no_client_auth());
        let mut tcp_stream = TcpStream::connect("discord.com:443").expect("api: connect error");
        let mut connection = ClientConnection::new(client_config, "discord.com".try_into().unwrap()).expect("ssl: context init failed");
        let mut stream: Stream<ClientConnection, TcpStream> = Stream::new(&mut connection, &mut tcp_stream);

        let mut form = FormData::new(Vec::new());

        form.write_file(
            "payload_json",
            Cursor::new(
                stringify!({
                    "content": "{content}",
                    "attachments": [
                        {
                            "id": 0,
                            "filename": "ProjBotV3.gif"
                        }
                    ]
                })
                .replace("{content}", content),
            ),
            None,
            "application/json",
        )
        .expect("form: payload_json failed");
        form.write_file(
            "files[0]",
            Cursor::new(self.bytes.as_slice()),
            Some(OsStr::new("ProjBotV3.gif")),
            "image/gif",
        )
        .expect("form: attachment failed");
        let mut data = form.finish().expect("form: finish failed");

        stream
            .write_all(
                format!(
                    "PATCH /api/v10/channels/{}/messages/{message} HTTP/1.1\n",
                    &self.channel
                )
                .as_bytes(),
            )
            .expect("api: write failed");
        stream
            .write_all(
                "Host: discord.com\nUser-Agent: ProjBotV3 image uploader (tudbut@tudbut.de)\n"
                    .as_bytes(),
            )
            .expect("api: write failed");
        stream
            .write_all(format!("Content-Length: {}\n", data.len()).as_bytes())
            .expect("api: write failed");
        stream
            .write_all(format!("Content-Type: {}\n", form.content_type_header()).as_bytes())
            .expect("api: write failed");
        stream
            .write_all(format!("Authorization: Bot {}\n\n", token).as_bytes())
            .expect("api: write failed");

        // remove the last byte and cache it in the frame object for later write finish
        self.byte_to_write = Some(
            *data
                .last()
                .expect("form: empty array returned (finish failed)"),
        );
        data.remove(data.len() - 1);

        stream
            .write_all(data.as_slice())
            .expect("api: write failed");
        stream.flush().expect("api: flush failed");

        self.cache_stream = Some(connection);
        self.tcp_stream = Some(tcp_stream);
        // now the frame is ready to send the next part
    }

    pub fn complete_send(&mut self) {
        let cache_stream = &mut self.cache_stream;
        let byte_to_write = &self.byte_to_write;
        let tcp_stream = &mut self.tcp_stream;
        if let Some(connection) = cache_stream {
            if let Some(byte) = byte_to_write {
                if let Some(tcp_stream) = tcp_stream {
                    let mut stream: Stream<ClientConnection, TcpStream> = Stream::new(connection, tcp_stream);
                    stream
                        .write_all(&[*byte])
                        .expect("api: write failed at complete_send");
                    stream.flush().expect("api: flush failed");
                    stream.sock
                        .set_read_timeout(Some(Duration::from_millis(500)))
                        .expect("tcp: unable to set timeout");
                    let mut buf = Vec::new();
                    let _ = stream.read_to_end(&mut buf); // failure is normal
                    stream.conn.send_close_notify();
                    stream.conn.write_tls(stream.sock).expect("ssl: unable to close connection");
                    stream.sock.flush().expect("ssl: unable to flush");
                    stream.sock
                        .shutdown(Shutdown::Both)
                        .expect("tcp: shutdown failed");
                    self.cache_stream = None;
                    self.byte_to_write = None;
                    return;
                }
            }
        }
        panic!("complete_send called on uncached frame!");
    }
}
