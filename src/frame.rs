use form_data_builder::FormData;
use openssl::ssl::{Ssl, SslContext, SslMethod, SslStream};
use std::{
    ffi::OsStr,
    io::{Cursor, Read, Write},
    net::{Shutdown, TcpStream},
    time::Duration,
};

pub struct Frame {
    pub bytes: Vec<u8>,
    pub channel: u64,
    cache_stream: Option<SslStream<TcpStream>>,
    byte_to_write: Option<u8>,
}

impl Frame {
    pub fn new(bytes: Vec<u8>, channel: u64) -> Frame {
        Frame {
            bytes,
            channel,
            cache_stream: None,
            byte_to_write: None,
        }
    }

    pub fn cache_frame(&mut self, message: u64, content: &str, token: &str) {
        let ssl_context = SslContext::builder(SslMethod::tls_client())
            .expect("ssl: context init failed")
            .build();
        let ssl = Ssl::new(&ssl_context).expect("ssl: init failed");
        let tcp_stream = TcpStream::connect("discord.com:443").expect("api: connect error");
        let mut stream = SslStream::new(ssl, tcp_stream).expect("ssl: stream init failed");

        let mut form = FormData::new(Vec::new());

        form.write_file(
            "payload_json",
            Cursor::new(
                stringify!({
                    "content": "{content}",
                    "attachments": [
                        {
                            "id": 0,
                            "filename": "projbot3.gif"
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
            Some(OsStr::new("projbot3.gif")),
            "image/gif",
        )
        .expect("form: attachment failed");
        let mut data = form.finish().expect("form: finish failed");

        stream.connect().expect("api: connection failed");
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
                "Host: discord.com\nUser-Agent: projbot3 image uploader (tudbut@tudbut.de)\n"
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

        self.cache_stream = Some(stream);
        // now the frame is ready to send the next part
    }

    pub fn complete_send(&mut self) {
        let cache_stream = &mut self.cache_stream;
        let byte_to_write = &self.byte_to_write;
        if let Some(stream) = cache_stream {
            if let Some(byte) = byte_to_write {
                stream
                    .write_all(&[*byte])
                    .expect("api: write failed at complete_send");
                stream.flush().expect("api: flush failed");
                stream
                    .get_ref()
                    .set_read_timeout(Some(Duration::from_millis(500)))
                    .expect("tcp: unable to set timeout");
                let mut buf = Vec::new();
                let _ = stream.read_to_end(&mut buf); // failure is normal
                stream.shutdown().expect("ssl: shutdown failed");
                stream
                    .get_ref()
                    .shutdown(Shutdown::Both)
                    .expect("tcp: shutdown failed");
                self.cache_stream = None;
                self.byte_to_write = None;
                return;
            }
        }
        panic!("complete_send called on uncached frame!");
    }
}
