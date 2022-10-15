mod frame;

use crate::frame::Frame;
use gif::Encoder;
use png::Decoder;
use serenity::{
    async_trait,
    framework::StandardFramework,
    futures::StreamExt,
    model::prelude::{ChannelId, ChannelType, Message},
    prelude::*,
    Client,
};
use songbird::SerenityInit;
use std::{
    env,
    fs::{self, File, OpenOptions},
    io::Read,
    path::Path,
    process::{self, Stdio},
    sync::{Arc, Mutex},
    thread,
    time::{Duration, SystemTime},
};

async fn send_video(message: Message, ctx: Context) {
    use tokio::sync::Mutex;

    // Read all frames from vid_encoded
    let mut v: Vec<Frame> = Vec::new();
    let dir = fs::read_dir("vid_encoded")
        .expect("unable to read dir")
        .count();
    for i in 0..dir {
        let mut file = OpenOptions::new()
            .read(true)
            .write(false)
            .create(false)
            .open(format!("vid_encoded/{i}"))
            .expect("readvid: invalid vid_encoded");
        let mut buf = Vec::new();
        file.read_to_end(&mut buf)
            .expect("readvid: unable to read file");
        v.push(Frame::new(buf, message.channel_id.0));
    }

    // Get serenity and songbird items
    let guild_id = message.guild_id.unwrap();
    let http = ctx.http.clone();
    let songbird = songbird::get(&ctx)
        .await
        .expect("voice: unable to initialize songbird");
    let c0: Arc<Mutex<Option<ChannelId>>> = Arc::new(Mutex::new(None));
    let c1 = c0.clone();

    // Spawn task to send video
    tokio::spawn(async move {
        let message = message;
        let ctx = ctx;
        let args = env::args().collect::<Vec<String>>();
        let token = args.get(1).unwrap().to_owned();
        let mut v = v.into_iter();
        let n = message
            .channel_id
            .say(
                &ctx.http,
                "<ProjBotV3 by TudbuT#2624> Image will appear below",
            )
            .await
            .expect("discord: unable to send");
        // Spawn task to send audio - This has to be here, because this is also where the timer is
        // started
        tokio::spawn(async move {
            let sa = unix_millis();
            println!("voice: init");
            let channel = guild_id
                .create_channel(http, |c| c.name("ProjBotV3-Sound").kind(ChannelType::Voice))
                .await
                .expect("voice: unable to create channel");
            let api_time = unix_millis() - sa;
            *c0.lock().await = Some(channel.id);
            println!("voice: joining");
            let (handler, err) = songbird.join(guild_id, channel.id).await;
            if let Err(e) = err {
                panic!("voice: error {e}");
            }
            println!("voice: loading");
            let handle = handler.lock().await.play_only_source(
                songbird::ffmpeg("aud_encoded")
                    .await
                    .expect("voice: unable to load"),
            );
            handle.make_playable().unwrap();
            handle.pause().expect("voice: unable to pause");
            handle.set_volume(1.0).unwrap();
            println!("voice: waiting for video [api_time={api_time}]");
            tokio::time::sleep(Duration::from_millis(
                5000 - (unix_millis() - sa)
                    + (api_time as i64
                        * i64::from_str_radix(
                            env::var("PROJBOTV3_API_TIME_FACTOR")
                                .unwrap_or("3".into())
                                .as_str(),
                            10,
                        )
                        .unwrap()) as u64,
            ))
            .await;
            println!("voice: playing");
            handle.play().expect("voice: unable to play");
        });

        // Initialize and start timing
        let mut sa = unix_millis();
        let mut to_compensate_for = 0;
        let mut free_time = 0;

        // Send frames (5 second long gifs)
        for mut frame in v.by_ref() {
            // Upload the frame to the API, but don't finish off the request.
            println!("vid: caching");
            let token = token.clone();
            let mut frame = tokio::task::spawn_blocking(move || {
                frame.cache_frame(
                    n.id.0,
                    format!("<ProjBotV3 by TudbuT#2624> Image will appear below [to_compensate_for={to_compensate_for}, free_time={free_time}]").as_str(),
                    token.as_str(),
                );
                frame
            }).await.unwrap();

            // Get recent messages
            let msgs = n
                .channel_id
                .messages_iter(&ctx.http)
                .take(30)
                .collect::<Vec<_>>()
                .await;

            // Do timing for good synchronization and commands
            println!("vid: waiting");
            let mut to_sleep = 5000 - ((unix_millis() - sa) as i128);
            // Check for commands (timing this is required because there are a few IO
            // operations being awaited)
            sa = unix_millis();
            if let Some(Ok(msg)) = msgs.iter().find(|x| x.as_ref().unwrap().content == "!stop") {
                msg.delete(&ctx.http)
                    .await
                    .expect("discord: unable to delete command");
                break;
            }
            if let Some(Ok(msg)) = msgs
                .iter()
                .find(|x| x.as_ref().unwrap().content == "!sync vid")
            {
                msg.delete(&ctx.http)
                    .await
                    .expect("discord: unable to delete command");
                to_compensate_for += 100;
                msg.channel_id
                    .say(
                        &ctx.http,
                        "<ProjBotV3 by TudbuT#2624> Skipped 100ms of video :+1:",
                    )
                    .await
                    .expect("discord: unable to send commannd response");
            }
            if let Some(Ok(msg)) = msgs
                .iter()
                .find(|x| x.as_ref().unwrap().content == "!sync aud")
            {
                msg.delete(&ctx.http)
                    .await
                    .expect("discord: unable to delete command");
                to_sleep += 100;
                msg.channel_id
                    .say(
                        &ctx.http,
                        "<ProjBotV3 by TudbuT#2624> Stretching 100ms of video :+1:",
                    )
                    .await
                    .expect("discord: unable to send commannd response");
            }
            to_sleep -= (unix_millis() - sa) as i128;
            // Now factor in to_compensate_for
            // Clippy doesn't like this, but it's the only way to do it in stable
            #[allow(clippy::never_loop)]
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
            // Set free_time to display
            free_time = to_sleep;
            tokio::time::sleep(Duration::from_millis(to_sleep as u64)).await;
            sa = unix_millis();

            // Now complete the request. This allows each request to take O(1) time
            println!("vid: completing");
            tokio::task::spawn_blocking(move || {
                frame.complete_send();
            })
            .await
            .unwrap();
        }

        // The last frame would immediately be deleted if we didn't wait here.
        tokio::time::sleep(Duration::from_millis(5000)).await;

        // Now clean up
        n.delete(&ctx.http)
            .await
            .expect("discord: unable to delete message");
        if let Some(c) = *c1.lock().await {
            c.delete(&ctx.http)
                .await
                .expect("discord: unable to delete voice channel");
        }
    });
}

struct Handler;

#[async_trait]
impl EventHandler for Handler {
    async fn message(&self, ctx: Context, message: Message) {
        if message.guild_id == None {
            return;
        }

        if message.content == "!play" {
            send_video(message, ctx).await;
        }
    }
}

#[tokio::main(flavor = "multi_thread", worker_threads = 10)]
async fn main() {
    // If vid_encoded doesn't exist, convert vid.mp4 into vid_encoded
    if !Path::new("vid_encoded/").is_dir() {
        println!("encode: encoding video...");
        if let Ok(_) = fs::create_dir("vid") {
            // We're using ffmpeg commands because ffmpeg's api is a hunk of junk
            let mut command = process::Command::new("ffmpeg")
                .args([
                    "-i",
                    "vid.mp4",
                    "-vf",
                    "fps=fps=25",
                    "-deadline",
                    "realtime",
                    "vid_25fps.mp4",
                ])
                .stdin(Stdio::inherit())
                .stdout(Stdio::inherit())
                .stderr(Stdio::inherit())
                .spawn()
                .expect("encode: unable to find or run ffmpeg");
            command.wait().expect("encode: ffmpeg failed: mp4->mp4");
            let mut command = process::Command::new("ffmpeg")
                .args([
                    "-i",
                    "vid_25fps.mp4",
                    "-vf",
                    "scale=240:180,setsar=1:1",
                    "-deadline",
                    "realtime",
                    "vid/%0d.png",
                ])
                .stdin(Stdio::inherit())
                .stdout(Stdio::inherit())
                .stderr(Stdio::inherit())
                .spawn()
                .expect("encode: unable to find or run ffmpeg");
            command.wait().expect("encode: ffmpeg failed: mp4->png");
            fs::remove_file("vid_25fps.mp4").expect("encode: rm vid_25fps.mp4 failed");
            let mut command = process::Command::new("ffmpeg")
                .args(["-i", "vid.mp4", "-deadline", "realtime", "aud.opus"])
                .stdin(Stdio::inherit())
                .stdout(Stdio::inherit())
                .stderr(Stdio::inherit())
                .spawn()
                .expect("encode: unable to find or run ffmpeg");
            command.wait().expect("encode: ffmpeg failed: mp4->opus");
            fs::rename("aud.opus", "aud_encoded")
                .expect("encode: unable to move aud.opus to aud_encoded");
        }
        // ffmpeg is now done converting vid.mp4 into vid/*.png

        // Create vid_encoded and encode gifs into it
        let _ = fs::create_dir("vid_encoded");
        let dir = fs::read_dir("vid")
            .expect("encode: unable to read files")
            .count();
        let running = Arc::new(Mutex::new(0));
        println!("encode: encoding gifs...");
        for n in 0..((dir as f32 / (25.0 * 5.0)).ceil() as usize) {
            *running.lock().unwrap() += 1;
            {
                let running = running.clone();
                // This thread will not interfere with tokio because it doesn't use anything async
                // and will exit before anything important is done in tokio.
                thread::spawn(move || {
                    let mut image = File::create(format!("vid_encoded/{n}"))
                        .expect("encode: unable to create gif file");
                    let mut encoder = Some(
                        Encoder::new(&mut image, 240, 180, &[])
                            .expect("encode: unable to create gif"),
                    );
                    // Write the gif control bytes
                    encoder
                        .as_mut()
                        .unwrap()
                        .write_extension(gif::ExtensionData::new_control_ext(
                            4,
                            gif::DisposalMethod::Any,
                            false,
                            None,
                        ))
                        .expect("encode: unable to write extension data");
                    encoder
                        .as_mut()
                        .unwrap()
                        .set_repeat(gif::Repeat::Finite(0))
                        .expect("encode: unable to set repeat");
                    // Encode frames into gif
                    println!("encode: encoding {n}...");
                    for i in (n * (25 * 5))..dir {
                        // n number of previously encoded gifs * 25 frames per second * 5 seconds
                        {
                            let i = i + 1; // because ffmpeg starts counting at 1 :p
                                           // Decode frame
                            let decoder = Decoder::new(
                                File::open(format!("vid/{i}.png"))
                                    .expect(format!("encode: unable to read vid/{i}.png").as_str()),
                            );
                            let mut reader = decoder.read_info().expect(
                                format!("encode: invalid ffmpeg output in vid/{i}.png").as_str(),
                            );
                            let mut buf: Vec<u8> = vec![0; reader.output_buffer_size()];
                            let info = reader.next_frame(&mut buf).expect(
                                format!("encode: invalid ffmpeg output in vid/{i}.png").as_str(),
                            );
                            let bytes = &mut buf[..info.buffer_size()];
                            // Encode frame
                            let mut frame = gif::Frame::from_rgb(240, 180, bytes);
                            // The gif crate is a little weird with extension data, it writes a
                            // block for each frame, so we have to remind it of what we want again
                            // for each frame
                            frame.delay = 4;
                            // Add to gif
                            encoder
                                .as_mut()
                                .unwrap()
                                .write_frame(&frame)
                                .expect("encode: unable to encode frame to gif");
                        }
                        // We don't want to encode something that is supposed to go into the next frame
                        if i / (25 * 5) != n {
                            break;
                        }
                    }
                    *running.lock().unwrap() -= 1;
                    println!("encode: encoded {n}");
                });
            }
            // Always have 6 running, but no more
            while *running.lock().unwrap() >= 6 {
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
            tokio::time::sleep(Duration::from_millis(500)).await;
        }
        while *running.lock().unwrap() != 0 {
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        println!("encode: done");
    }

    // Start the discord bot
    let framework = StandardFramework::new().configure(|c| c.prefix("!"));
    let mut client = Client::builder(
        env::args()
            .collect::<Vec<String>>()
            .get(1)
            .expect("discord: no token provided"),
        GatewayIntents::non_privileged()
            | GatewayIntents::MESSAGE_CONTENT
            | GatewayIntents::GUILD_VOICE_STATES,
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

// Helper function to get millis from unix epoch as a u64
fn unix_millis() -> u64 {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64
}
