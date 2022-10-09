use std::{
    env,
    ffi::OsStr,
    fs::{self, OpenOptions},
    io::{Cursor, Read, Write},
    net::{Shutdown, TcpStream},
    time::{Duration, SystemTime}, sync::Arc,
};

use form_data_builder::FormData;
use openssl::ssl::{Ssl, SslContext, SslMethod, SslStream};
use serenity::{
    async_trait,
    framework::StandardFramework,
    model::prelude::{ChannelType, Message, ChannelId},
    prelude::*,
    Client, futures::StreamExt,
};
use songbird::SerenityInit;

struct Frame {
    bytes: Vec<u8>,
    channel: u64,
    cache_stream: Option<SslStream<TcpStream>>,
    byte_to_write: Option<u8>,
}

impl Frame {
    fn cache_frame(&mut self, message: u64, content: &str, token: &str) {
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
            .write(
                format!(
                    "PATCH /api/v10/channels/{}/messages/{message} HTTP/1.1\n",
                    &self.channel
                )
                .as_bytes(),
            )
            .expect("api: write failed");
        stream
            .write(
                "Host: discord.com\nUser-Agent: projbot3 image uploader (tudbut@tudbut.de)\n"
                    .as_bytes(),
            )
            .expect("api: write failed");
        stream
            .write(format!("Content-Length: {}\n", data.len()).as_bytes())
            .expect("api: write failed");
        stream
            .write(format!("Content-Type: {}\n", form.content_type_header()).as_bytes())
            .expect("api: write failed");
        stream
            .write(format!("Authorization: Bot {}\n\n", token).as_bytes())
            .expect("api: write failed");

        // remove the last byte and cache it in the frame object for later write finish
        self.byte_to_write = Some(
            *data
                .last()
                .expect("form: empty array returned (finish failed)"),
        );
        data.remove(data.len() - 1);

        stream.write(data.as_slice()).expect("api: write failed");

        self.cache_stream = Some(stream);
        // now the frame is ready to send the next part
    }

    fn complete_send(&mut self) {
        let ref mut cache_stream = self.cache_stream;
        let ref byte_to_write = self.byte_to_write;
        if let Some(stream) = cache_stream {
            if let Some(byte) = byte_to_write {
                stream
                    .write(&[*byte])
                    .expect("api: write failed at complete_send");
                stream.get_ref().set_read_timeout(Some(Duration::from_millis(500))).expect("tcp: unable to set timeout");
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

async fn send_frames(message: Message, ctx: Context) {
    let mut v: Vec<Frame> = Vec::new();
    let dir = fs::read_dir("vid_encoded").expect("unable to read dir");
    let dir: Vec<_> = dir.collect();
    for i in 0..dir.len() {
        let mut file = OpenOptions::new()
            .read(true)
            .write(false)
            .create(false)
            .open(format!("vid_encoded/{i}"))
            .expect("readvid: invalid vid_encoded");
        let mut buf = Vec::new();
        file.read_to_end(&mut buf)
            .expect("readvid: unable to read file");
        v.push(Frame {
            bytes: buf,
            channel: message.channel_id.0,
            cache_stream: None,
            byte_to_write: None,
        });
    }
    let guild_id = message.guild_id.unwrap();
    let http = ctx.http.clone();
    let songbird = songbird::get(&ctx)
        .await
        .expect("voice: unable to initialize songbird");
    let c0: Arc<Mutex<Option<ChannelId>>> = Arc::new(Mutex::new(None));
    let c1 = c0.clone();
    //thread::spawn(move || {tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap().block_on(async move {
    tokio::spawn(async move {
        let message = message;
        let ctx = ctx;
        let args = env::args().collect::<Vec<String>>();
        let token = args.get(1).unwrap();
        let mut v = v.into_iter();
        let n = message
            .channel_id
            .say(
                &ctx.http,
                "<ProjBotV3 by TudbuT#2624> Image will appear below",
            )
            .await
            .expect("discord: unable to send");
        println!("starting to send in {}@{}", n.id.0, message.channel_id.0);
        //thread::spawn(move || {tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap().block_on(async move {
        tokio::spawn(async move {
            let sa = unix_millis();
            println!("voice: init");
            let channel = guild_id.create_channel(http, |c| c.name("ProjBotV3-Sound").kind(ChannelType::Voice)).await.expect("voice: unable to create channel");
            *c0.lock().await = Some(channel.id);
            println!("voice: joining");
            let (handler, err) = songbird.join(guild_id, channel.id).await;
            if let Err(e) = err {
                panic!("voice: error {e}");
            }
            println!("voice: loading");
            let handle = handler.lock().await.play_source(songbird::ffmpeg("aud_encoded").await.expect("voice: unable to load"));
            handle.make_playable().unwrap();
            handle.pause().expect("voice: unable to pause");
            handle.set_volume(1.0).unwrap();
            println!("voice: waiting for video");
            tokio::time::sleep(Duration::from_millis(5000 - (unix_millis() - sa))).await;
            println!("voice: playing");
            handle.play().expect("voice: unable to play");
            println!("{:?}", handle.get_info().await);
        });//});
        let mut sa = unix_millis();
        let mut to_compensate_for = 0;
        while let Some(mut frame) = v.next() {
            println!("vid: caching");
            frame.cache_frame(
                n.id.0,
                format!("<ProjBotV3 by TudbuT#2624> Image will appear below [to_compensate_for={to_compensate_for}]").as_str(),
                token,
            );
            let msgs = n.channel_id.messages_iter(&ctx.http).take(30).collect::<Vec<_>>().await;
            println!("vid: waiting");
            let mut to_sleep = 5000 - ((unix_millis() - sa) as i128);
            sa = unix_millis();
            if let Some(Ok(msg)) = msgs.iter().filter(|x| x.as_ref().unwrap().content == "!stop").next() {
                msg.delete(&ctx.http).await.expect("discord: unable to delete command");
                break;
            }
            if let Some(Ok(msg)) = msgs.iter().filter(|x| x.as_ref().unwrap().content == "!sync vid").next() {
                msg.delete(&ctx.http).await.expect("discord: unable to delete command");
                to_compensate_for += 100;
                msg.channel_id.say(&ctx.http, "<ProjBotV3 by TudbuT#2624> Skipped 100ms of video :+1:").await.expect("discord: unable to send commannd response");
            }
            if let Some(Ok(msg)) = msgs.iter().filter(|x| x.as_ref().unwrap().content == "!sync aud").next() {
                msg.delete(&ctx.http).await.expect("discord: unable to delete command");
                to_sleep += 100;
                msg.channel_id.say(&ctx.http, "<ProjBotV3 by TudbuT#2624> Skipped 100ms of video :+1:").await.expect("discord: unable to send commannd response");
            }
            to_sleep -= (unix_millis() - sa) as i128;
            'calc: loop {
                if to_sleep < 0 {
                    to_compensate_for += -to_sleep;
                    break 'calc;
                }

                if to_compensate_for > 0 {
                    if to_sleep - to_compensate_for >= 0 {
                        to_sleep -= to_compensate_for;
                        to_compensate_for = 0;
                    } else {
                        to_compensate_for -= to_sleep;
                        to_sleep = 0;
                    }
                    break 'calc;
                }

                break 'calc;
            }
            tokio::time::sleep(Duration::from_millis(to_sleep as u64)).await;
            sa = unix_millis();
            println!("vid: completing");
            frame.complete_send();
        }
        n.delete(&ctx.http)
            .await
            .expect("discord: unable to delete message");
        if let Some(c) = *c1.lock().await {
            c.delete(&ctx.http).await.expect("discord: unable to delete voice channel");
        }
    });//});
}

struct Handler;

#[async_trait]
impl EventHandler for Handler {

    async fn message(&self, ctx: Context, message: Message) {
        if message.guild_id == None {
            println!("DM");
            return;
        }

        if message.content == "!play" {
            println!("hi");
            send_frames(message, ctx).await;
        }
    }
}

#[tokio::main]
async fn main() {
    let framework = StandardFramework::new().configure(|c| c.prefix("!"));
    let mut client = Client::builder(
        env::args()
            .collect::<Vec<String>>()
            .get(1)
            .expect("discord: no token provided"),
        GatewayIntents::non_privileged() | GatewayIntents::MESSAGE_CONTENT | GatewayIntents::GUILD_VOICE_STATES,
    )
    .framework(framework)
    .event_handler(Handler)
    .register_songbird()
    .await
    .expect("discord: init failed");

    if let Err(why) = client.start().await {
        println!("An error occurred while running the client: {:?}", why);
    }
}

fn unix_millis() -> u64 {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64
}