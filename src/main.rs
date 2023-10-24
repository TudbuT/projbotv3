#![allow(clippy::expect_fun_call)] // I don't see a reason for this warn

mod convert;
mod frame;

use crate::convert::*;
use crate::frame::*;
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
    fs::{self, OpenOptions},
    io::Read,
    path::Path,
    sync::Arc,
    time::{Duration, SystemTime},
};
use tokio::sync::Mutex;

async fn send_video(message: Message, ctx: Context) {
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
        let sa = unix_millis();
        println!("voice: init");
        let channel = guild_id
            .create_channel(http, |c| c.name("ProjBotV3-Sound").kind(ChannelType::Voice))
            .await
            .expect("voice: unable to create channel");
        let api_time = unix_millis() - sa;
        tokio::spawn(async move {
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
                        * str::parse::<i64>(
                            env::var("PROJBOTV3_API_TIME_FACTOR")
                                .unwrap_or_else(|_| "5".into())
                                .as_str(),
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

        const MB_1: usize = 1024 * 1024;
        // Send frames (5 second long gifs)
        for mut frame in v.by_ref() {
            let size_mb = frame.bytes.len() as f32 / MB_1 as f32;
            let size_compensation_active = frame.bytes.len() > MB_1;

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
            let size_compensation_time = if size_compensation_active {
                (api_time as f32 * size_mb) as u64
            } else {
                0
            };
            tokio::time::sleep(Duration::from_millis(
                to_sleep as u64 - size_compensation_time,
            ))
            .await;
            sa = unix_millis() + size_compensation_time;

            // Now complete the request. This allows each request to take O(1) time
            println!("vid: completing");
            tokio::task::spawn_blocking(move || {
                frame.complete_send();
            })
            .await
            .unwrap();
            tokio::time::sleep(Duration::from_millis(size_compensation_time)).await;
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

// Unit struct used as event handler
struct Handler;

#[async_trait]
impl EventHandler for Handler {
    async fn message(&self, ctx: Context, message: Message) {
        if message.guild_id.is_none() {
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
        convert().await;
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
